//! Optional, provider-neutral weather input and cache orchestration.
//!
//! Weather is deliberately kept outside the Wayland/rendering path.  This
//! module normalizes provider results, persists only a bounded last-known
//! value, and offers a worker-backed service with one request in flight.  It
//! never selects a scene: completions only update the cache and diagnostics.

#![allow(dead_code)]

use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{environment::Location, scene_registry::Weather};

pub const CACHE_SCHEMA_VERSION: u32 = 1;
pub const NORMAL_POLL_INTERVAL: Duration = Duration::from_secs(15 * 60);
pub const DEFAULT_STALE_AFTER: Duration = Duration::from_secs(60 * 60);
pub const DEFAULT_REQUEST_DEADLINE: Duration = Duration::from_secs(10);
pub const MAX_CACHE_BYTES: usize = 16 * 1024;
pub const MAX_SOURCE_BYTES: usize = 128;
const MAX_FUTURE_SKEW_SECONDS: i64 = 5 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherError(String);

impl fmt::Display for WeatherError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for WeatherError {}

fn error(message: impl Into<String>) -> WeatherError {
    WeatherError(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCondition {
    Clear,
    Cloudy,
    Rain,
    Snow,
    Storm,
    Fog,
    Unknown,
}

impl ProviderCondition {
    fn weather(self) -> Weather {
        match self {
            Self::Clear => Weather::Clear,
            Self::Cloudy => Weather::Cloudy,
            Self::Rain => Weather::Rain,
            Self::Snow => Weather::Snow,
            Self::Storm => Weather::Storm,
            Self::Fog => Weather::Fog,
            Self::Unknown => Weather::Unknown,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Cloudy => "cloudy",
            Self::Rain => "rain",
            Self::Snow => "snow",
            Self::Storm => "storm",
            Self::Fog => "fog",
            Self::Unknown => "unknown",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "clear" => Self::Clear,
            "cloudy" => Self::Cloudy,
            "rain" => Self::Rain,
            "snow" => Self::Snow,
            "storm" => Self::Storm,
            "fog" => Self::Fog,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderPayload {
    pub condition: ProviderCondition,
    pub temperature_c: Option<f32>,
    pub precipitation: bool,
    pub observed_at_utc: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherReading {
    pub condition: Weather,
    pub temperature_c: Option<f32>,
    pub precipitation: bool,
    pub observed_at_utc: i64,
    pub fetched_at_utc: i64,
    pub source: String,
}

impl WeatherReading {
    pub fn from_provider(
        payload: ProviderPayload,
        fetched_at_utc: i64,
        source: impl Into<String>,
    ) -> Result<Self, WeatherError> {
        let source = source.into();
        validate_source(&source)?;
        if let Some(temperature) = payload.temperature_c {
            if !temperature.is_finite() || !(-100.0..=100.0).contains(&temperature) {
                return Err(error(
                    "provider temperature is not a finite Celsius value in range",
                ));
            }
        }
        if payload.observed_at_utc > fetched_at_utc.saturating_add(MAX_FUTURE_SKEW_SECONDS) {
            return Err(error(
                "provider observation timestamp is too far in the future",
            ));
        }
        Ok(Self {
            condition: payload.condition.weather(),
            temperature_c: payload.temperature_c,
            precipitation: payload.precipitation,
            observed_at_utc: payload.observed_at_utc,
            fetched_at_utc,
            source,
        })
    }

    fn validate(&self, now_utc: i64) -> Result<(), WeatherError> {
        if self.fetched_at_utc > now_utc.saturating_add(MAX_FUTURE_SKEW_SECONDS) {
            return Err(error("weather cache fetched timestamp is in the future"));
        }
        if self.observed_at_utc > self.fetched_at_utc.saturating_add(MAX_FUTURE_SKEW_SECONDS) {
            return Err(error("weather cache observed timestamp is in the future"));
        }
        if let Some(temperature) = self.temperature_c {
            if !temperature.is_finite() || !(-100.0..=100.0).contains(&temperature) {
                return Err(error("weather cache temperature is invalid"));
            }
        }
        validate_source(&self.source)
    }
}

fn validate_source(source: &str) -> Result<(), WeatherError> {
    if source.is_empty() || source.len() > MAX_SOURCE_BYTES {
        return Err(error("weather source is empty or too long"));
    }
    if !source.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
    }) {
        return Err(error("weather source contains an invalid character"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Unknown,
    Fresh,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherState {
    Disabled,
    Unknown,
    Fresh,
    Stale,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherSnapshot {
    pub reading: Option<WeatherReading>,
    pub freshness: Freshness,
    pub state: WeatherState,
    pub last_error: Option<String>,
}

impl WeatherSnapshot {
    pub fn weather(&self) -> Weather {
        self.reading
            .as_ref()
            .map_or(Weather::Unknown, |reading| reading.condition)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherCache {
    reading: WeatherReading,
}

impl WeatherCache {
    pub fn new(reading: WeatherReading, now_utc: i64) -> Result<Self, WeatherError> {
        reading.validate(now_utc)?;
        Ok(Self { reading })
    }

    pub fn reading(&self) -> &WeatherReading {
        &self.reading
    }

    pub fn freshness(&self, now_utc: i64, stale_after: Duration) -> Freshness {
        let age = now_utc.saturating_sub(self.reading.fetched_at_utc);
        if age < 0 {
            Freshness::Unknown
        } else if age <= stale_after.as_secs() as i64 {
            Freshness::Fresh
        } else {
            Freshness::Stale
        }
    }

    fn replace(&mut self, reading: WeatherReading, now_utc: i64) -> Result<(), WeatherError> {
        reading.validate(now_utc)?;
        self.reading = reading;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheStore {
    path: PathBuf,
}

impl CacheStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_path() -> Result<PathBuf, WeatherError> {
        let root = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .ok_or_else(|| error("XDG_CACHE_HOME or HOME is required for weather cache"))?;
        Ok(root.join("worldwall").join("weather.cache"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self, now_utc: i64) -> Result<Option<WeatherCache>, WeatherError> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error_value) if error_value.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error_value) => return Err(io_error("opening weather cache", error_value)),
        };
        let length = file
            .metadata()
            .map(|metadata| metadata.len() as usize)
            .unwrap_or(MAX_CACHE_BYTES + 1);
        if length > MAX_CACHE_BYTES {
            return Err(error("weather cache exceeds the bounded size"));
        }
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|error_value| io_error("reading weather cache", error_value))?;
        let reading = parse_cache(&contents)?;
        Ok(Some(WeatherCache::new(reading, now_utc)?))
    }

    pub fn save(&self, cache: &WeatherCache, now_utc: i64) -> Result<(), WeatherError> {
        cache.reading.validate(now_utc)?;
        let contents = serialize_cache(&cache.reading);
        if contents.len() > MAX_CACHE_BYTES {
            return Err(error("serialized weather cache exceeds the bounded size"));
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| error("weather cache path has no parent"))?;
        fs::create_dir_all(parent)
            .map_err(|error_value| io_error("creating weather cache directory", error_value))?;
        restrict_directory(parent)?;

        let temporary = parent.join(format!(
            ".weather.cache.{}.{}",
            std::process::id(),
            unique_suffix()
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error_value| io_error("creating temporary weather cache", error_value))?;
            restrict_file(&file)?;
            file.write_all(contents.as_bytes())
                .map_err(|error_value| io_error("writing temporary weather cache", error_value))?;
            file.sync_all()
                .map_err(|error_value| io_error("syncing temporary weather cache", error_value))?;
            fs::rename(&temporary, &self.path).map_err(|error_value| {
                io_error("atomically replacing weather cache", error_value)
            })?;
            restrict_path(&self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn io_error(action: &str, value: io::Error) -> WeatherError {
    error(format!("{action} failed: {value}"))
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), WeatherError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error_value| io_error("restricting weather cache directory", error_value))
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), WeatherError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file(file: &File) -> Result<(), WeatherError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error_value| io_error("restricting weather cache file", error_value))
}

#[cfg(not(unix))]
fn restrict_file(_file: &File) -> Result<(), WeatherError> {
    Ok(())
}

fn restrict_path(path: &Path) -> Result<(), WeatherError> {
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error_value| io_error("opening weather cache for permissions", error_value))?;
    restrict_file(&file)
}

fn serialize_cache(reading: &WeatherReading) -> String {
    format!(
        "schema_version={}\ncondition={}\ntemperature_c={}\nprecipitation={}\nobserved_at_utc={}\nfetched_at_utc={}\nsource={}\n",
        CACHE_SCHEMA_VERSION,
        reading.condition.to_cache_str(),
        reading
            .temperature_c
            .map_or_else(|| "none".to_owned(), |value| format!("{value:.4}")),
        reading.precipitation,
        reading.observed_at_utc,
        reading.fetched_at_utc,
        reading.source,
    )
}

fn parse_cache(input: &str) -> Result<WeatherReading, WeatherError> {
    let mut values = std::collections::BTreeMap::new();
    for line in input.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| error("weather cache contains a malformed line"))?;
        if values.insert(key, value).is_some() {
            return Err(error("weather cache contains a duplicate field"));
        }
    }
    let version = required(&values, "schema_version")?
        .parse::<u32>()
        .map_err(|_| error("weather cache schema version is invalid"))?;
    if version != CACHE_SCHEMA_VERSION {
        return Err(error(format!(
            "unsupported weather cache schema version {version}"
        )));
    }
    let condition = Weather::parse_cache(required(&values, "condition")?)?;
    let temperature_c = match required(&values, "temperature_c")? {
        "none" => None,
        value => Some(
            value
                .parse::<f32>()
                .map_err(|_| error("weather cache temperature is invalid"))?,
        ),
    };
    let precipitation = match required(&values, "precipitation")? {
        "true" => true,
        "false" => false,
        _ => return Err(error("weather cache precipitation is invalid")),
    };
    let observed_at_utc = required(&values, "observed_at_utc")?
        .parse::<i64>()
        .map_err(|_| error("weather cache observed timestamp is invalid"))?;
    let fetched_at_utc = required(&values, "fetched_at_utc")?
        .parse::<i64>()
        .map_err(|_| error("weather cache fetched timestamp is invalid"))?;
    let source = required(&values, "source")?.to_owned();
    Ok(WeatherReading {
        condition,
        temperature_c,
        precipitation,
        observed_at_utc,
        fetched_at_utc,
        source,
    })
}

fn required<'a>(
    values: &'a std::collections::BTreeMap<&str, &str>,
    key: &str,
) -> Result<&'a str, WeatherError> {
    values
        .get(key)
        .copied()
        .ok_or_else(|| error(format!("weather cache is missing {key}")))
}

impl Weather {
    fn to_cache_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Cloudy => "cloudy",
            Self::Rain => "rain",
            Self::Snow => "snow",
            Self::Storm => "storm",
            Self::Fog => "fog",
            Self::Unknown => "unknown",
        }
    }

    fn parse_cache(value: &str) -> Result<Self, WeatherError> {
        Ok(match value {
            "clear" => Self::Clear,
            "cloudy" => Self::Cloudy,
            "rain" => Self::Rain,
            "snow" => Self::Snow,
            "storm" => Self::Storm,
            "fog" => Self::Fog,
            "unknown" => Self::Unknown,
            _ => return Err(error("weather cache condition is invalid")),
        })
    }
}

#[derive(Debug, Clone)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub location: Option<Location>,
    pub source: String,
    pub poll_interval: Duration,
    pub stale_after: Duration,
    pub request_deadline: Duration,
    pub retry_backoff: Vec<Duration>,
}

impl WeatherConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            location: None,
            source: String::new(),
            poll_interval: NORMAL_POLL_INTERVAL,
            stale_after: DEFAULT_STALE_AFTER,
            request_deadline: DEFAULT_REQUEST_DEADLINE,
            retry_backoff: vec![Duration::from_secs(5), Duration::from_secs(30)],
        }
    }

    pub fn enabled(location: Location, source: impl Into<String>) -> Result<Self, WeatherError> {
        let source = source.into();
        validate_source(&source)?;
        Ok(Self {
            enabled: true,
            location: Some(location),
            source,
            poll_interval: NORMAL_POLL_INTERVAL,
            stale_after: DEFAULT_STALE_AFTER,
            request_deadline: DEFAULT_REQUEST_DEADLINE,
            retry_backoff: vec![Duration::from_secs(5), Duration::from_secs(30)],
        })
    }

    pub fn open_meteo(location: Location) -> Result<Self, WeatherError> {
        Self::enabled(location, OPEN_METEO_SOURCE)
    }

    fn validate(&self) -> Result<(), WeatherError> {
        if !self.enabled {
            return Ok(());
        }
        if self.location.is_none() {
            return Err(error("enabled weather requires an explicit location"));
        }
        validate_source(&self.source)?;
        if self.poll_interval < NORMAL_POLL_INTERVAL {
            return Err(error(
                "weather poll interval is shorter than the 15-minute policy",
            ));
        }
        if self.request_deadline.is_zero() {
            return Err(error("weather request deadline must be nonzero"));
        }
        if self.retry_backoff.len() > 3 {
            return Err(error("weather retry count exceeds the bounded limit"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub location: Location,
    pub requested_at_utc: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorKind {
    Network,
    Timeout,
    Authorization,
    RateLimited,
    InvalidPayload,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
}

const OPEN_METEO_ENDPOINT: &str = "https://api.open-meteo.com/v1/forecast";
const OPEN_METEO_SOURCE: &str = "open-meteo.com";
const MAX_PROVIDER_RESPONSE_BYTES: usize = 64 * 1024;

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

/// Open-Meteo's free forecast endpoint exposes current values as documented at
/// https://open-meteo.com/en/docs. The request asks only for Celsius current
/// temperature, precipitation in millimetres, WMO weather code, and a Unix
/// timestamp in GMT. No API key or location lookup is performed here.
#[derive(Debug, Clone)]
pub struct OpenMeteoProvider {
    client: reqwest::blocking::Client,
    endpoint: String,
}

impl OpenMeteoProvider {
    pub fn new(timeout: Duration) -> Result<Self, WeatherError> {
        Self::with_endpoint(timeout, OPEN_METEO_ENDPOINT)
    }

    fn with_endpoint(timeout: Duration, endpoint: impl Into<String>) -> Result<Self, WeatherError> {
        if timeout.is_zero() {
            return Err(error("Open-Meteo timeout must be nonzero"));
        }
        let endpoint = endpoint.into();
        if endpoint.is_empty() {
            return Err(error("Open-Meteo endpoint must be nonempty"));
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent("worldwall/0.1")
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()
            .map_err(|error_value| {
                error(format!("creating Open-Meteo client failed: {error_value}"))
            })?;
        Ok(Self { client, endpoint })
    }

    pub fn endpoint() -> &'static str {
        OPEN_METEO_ENDPOINT
    }

    fn request_url(&self, location: Location) -> String {
        format!(
            "{}?latitude={:.6}&longitude={:.6}&current=temperature_2m,precipitation,weather_code&temperature_unit=celsius&timeformat=unixtime&timezone=GMT",
            self.endpoint,
            location.latitude_degrees(),
            location.longitude_degrees()
        )
    }
}

impl WeatherProvider for OpenMeteoProvider {
    fn fetch(
        &self,
        request: ProviderRequest,
        cancellation: &CancellationToken,
    ) -> Result<ProviderPayload, ProviderError> {
        if cancellation.is_cancelled() {
            return Err(provider_error(
                ProviderErrorKind::Cancelled,
                "Open-Meteo request cancelled",
            ));
        }
        let response = self
            .client
            .get(self.request_url(request.location))
            .send()
            .map_err(|error_value| {
                provider_error(
                    if error_value.is_timeout() {
                        ProviderErrorKind::Timeout
                    } else {
                        ProviderErrorKind::Network
                    },
                    if error_value.is_timeout() {
                        "Open-Meteo request timed out"
                    } else {
                        "Open-Meteo request failed"
                    },
                )
            })?;
        let status = response.status();
        if !status.is_success() {
            let kind = classify_http_status(status.as_u16());
            return Err(provider_error(
                kind,
                format!("Open-Meteo returned HTTP {}", status.as_u16()),
            ));
        }
        let mut body = Vec::new();
        response
            .take((MAX_PROVIDER_RESPONSE_BYTES + 1) as u64)
            .read_to_end(&mut body)
            .map_err(|_| {
                provider_error(
                    ProviderErrorKind::Network,
                    "reading Open-Meteo response failed",
                )
            })?;
        if body.len() > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(provider_error(
                ProviderErrorKind::InvalidPayload,
                "Open-Meteo response exceeds the bounded size",
            ));
        }
        if cancellation.is_cancelled() {
            return Err(provider_error(
                ProviderErrorKind::Cancelled,
                "Open-Meteo request cancelled",
            ));
        }
        parse_open_meteo_response(std::str::from_utf8(&body).map_err(|_| {
            provider_error(
                ProviderErrorKind::InvalidPayload,
                "Open-Meteo response is not UTF-8",
            )
        })?)
    }
}

fn provider_error(kind: ProviderErrorKind, message: impl Into<String>) -> ProviderError {
    ProviderError {
        kind,
        message: message.into(),
    }
}

fn classify_http_status(status: u16) -> ProviderErrorKind {
    match status {
        401 | 403 => ProviderErrorKind::Authorization,
        408 | 504 => ProviderErrorKind::Timeout,
        429 => ProviderErrorKind::RateLimited,
        _ => ProviderErrorKind::Network,
    }
}

pub fn parse_open_meteo_response(input: &str) -> Result<ProviderPayload, ProviderError> {
    let root: serde_json::Value = serde_json::from_str(input).map_err(|_| {
        provider_error(
            ProviderErrorKind::InvalidPayload,
            "Open-Meteo response is not valid JSON",
        )
    })?;
    if root.get("error").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(provider_error(
            ProviderErrorKind::InvalidPayload,
            "Open-Meteo reported an API error",
        ));
    }
    let current = root.get("current").ok_or_else(|| {
        provider_error(
            ProviderErrorKind::InvalidPayload,
            "Open-Meteo response has no current conditions",
        )
    })?;
    let temperature = current
        .get("temperature_2m")
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| {
            provider_error(
                ProviderErrorKind::InvalidPayload,
                "Open-Meteo temperature is missing",
            )
        })?;
    let precipitation = current
        .get("precipitation")
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| {
            provider_error(
                ProviderErrorKind::InvalidPayload,
                "Open-Meteo precipitation is missing",
            )
        })?;
    let weather_code = current
        .get("weather_code")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| {
            provider_error(
                ProviderErrorKind::InvalidPayload,
                "Open-Meteo weather code is missing",
            )
        })?;
    let observed_at_utc = current
        .get("time")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| {
            provider_error(
                ProviderErrorKind::InvalidPayload,
                "Open-Meteo current time is missing",
            )
        })?;
    if !temperature.is_finite() || !precipitation.is_finite() || precipitation < 0.0 {
        return Err(provider_error(
            ProviderErrorKind::InvalidPayload,
            "Open-Meteo current values are not finite and non-negative",
        ));
    }
    Ok(ProviderPayload {
        condition: condition_from_wmo_code(weather_code),
        temperature_c: Some(temperature as f32),
        precipitation: precipitation > 0.0,
        observed_at_utc,
    })
}

fn condition_from_wmo_code(code: i64) -> ProviderCondition {
    match code {
        0 => ProviderCondition::Clear,
        1..=3 => ProviderCondition::Cloudy,
        45 | 48 => ProviderCondition::Fog,
        51..=67 | 80..=82 => ProviderCondition::Rain,
        71..=77 | 85..=86 => ProviderCondition::Snow,
        95..=99 => ProviderCondition::Storm,
        _ => ProviderCondition::Unknown,
    }
}

pub trait WeatherProvider: Send + Sync + 'static {
    fn fetch(
        &self,
        request: ProviderRequest,
        cancellation: &CancellationToken,
    ) -> Result<ProviderPayload, ProviderError>;
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshKind {
    Startup,
    Resume,
    Manual,
    Poll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Disabled,
    Started { attempt: u8 },
    InFlight,
    RateLimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherDiagnostics {
    pub in_flight: bool,
    pub cache_epoch: u64,
    pub retry_attempt: u8,
}

struct FetchJob {
    request: ProviderRequest,
    fetched_at_utc: i64,
    token: CancellationToken,
    attempt: u8,
}

struct FetchCompletion {
    fetched_at_utc: i64,
    token: CancellationToken,
    attempt: u8,
    result: Result<ProviderPayload, ProviderError>,
}

struct InFlight {
    token: CancellationToken,
    started: Instant,
    fetched_at_utc: i64,
    attempt: u8,
}

pub struct WeatherService {
    config: WeatherConfig,
    jobs: Option<SyncSender<FetchJob>>,
    completions: Receiver<FetchCompletion>,
    in_flight: Option<InFlight>,
    cache: Option<WeatherCache>,
    cache_store: Option<CacheStore>,
    next_allowed_utc: Option<i64>,
    retry_attempt: u8,
    cache_epoch: u64,
    last_error: Option<String>,
}

impl WeatherService {
    pub fn new<P>(config: WeatherConfig, provider: P) -> Result<Self, WeatherError>
    where
        P: WeatherProvider,
    {
        config.validate()?;
        let (completions_tx, completions_rx) = mpsc::sync_channel(4);
        let jobs = if config.enabled {
            let (jobs_tx, jobs_rx) = mpsc::sync_channel::<FetchJob>(1);
            thread::Builder::new()
                .name("worldwall-weather".to_owned())
                .spawn(move || {
                    while let Ok(job) = jobs_rx.recv() {
                        let token = job.token.clone();
                        let result = if token.is_cancelled() {
                            Err(ProviderError {
                                kind: ProviderErrorKind::Cancelled,
                                message: "weather request cancelled".to_owned(),
                            })
                        } else {
                            provider.fetch(job.request, &token)
                        };
                        if completions_tx
                            .send(FetchCompletion {
                                fetched_at_utc: job.fetched_at_utc,
                                token,
                                attempt: job.attempt,
                                result,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .map_err(|error_value| {
                    error(format!("starting weather worker failed: {error_value}"))
                })?;
            Some(jobs_tx)
        } else {
            None
        };
        Ok(Self {
            config,
            jobs,
            completions: completions_rx,
            in_flight: None,
            cache: None,
            cache_store: None,
            next_allowed_utc: None,
            retry_attempt: 0,
            cache_epoch: 0,
            last_error: None,
        })
    }

    pub fn attach_cache(&mut self, store: CacheStore, now_utc: i64) -> Result<(), WeatherError> {
        self.cache_store = Some(store);
        let store = self
            .cache_store
            .as_ref()
            .expect("cache store was just installed");
        self.cache = store.load(now_utc)?;
        Ok(())
    }

    /// Attach a cache without making a corrupt or unreadable file fatal to
    /// startup. The store remains installed so the first valid provider
    /// result can replace the bad file atomically. A rejected cache is
    /// reported through the normal weather error state until that replacement
    /// succeeds.
    pub fn attach_cache_best_effort(
        &mut self,
        store: CacheStore,
        now_utc: i64,
    ) -> Result<(), WeatherError> {
        self.cache_store = Some(store);
        let store = self
            .cache_store
            .as_ref()
            .expect("cache store was just installed");
        match store.load(now_utc) {
            Ok(cache) => {
                self.cache = cache;
                Ok(())
            }
            Err(error_value) => {
                // A bad on-disk value must not displace a valid in-memory
                // snapshot installed by an earlier refresh.
                self.last_error = Some(format!("weather cache rejected: {error_value}"));
                Ok(())
            }
        }
    }

    pub fn install_cache(&mut self, cache: WeatherCache) {
        self.cache = Some(cache);
    }

    pub fn request_refresh(
        &mut self,
        now_utc: i64,
        now_mono: Instant,
        kind: RefreshKind,
    ) -> RefreshOutcome {
        if !self.config.enabled {
            return RefreshOutcome::Disabled;
        }
        if self.in_flight.is_some() {
            return RefreshOutcome::InFlight;
        }
        if self.next_allowed_utc.is_some_and(|next| now_utc < next) {
            return RefreshOutcome::RateLimited;
        }
        let Some(location) = self.config.location else {
            self.last_error = Some("weather location is not configured".to_owned());
            return RefreshOutcome::Disabled;
        };
        let attempt = self.retry_attempt.saturating_add(1);
        let token = CancellationToken::default();
        let job = FetchJob {
            request: ProviderRequest {
                location,
                requested_at_utc: now_utc,
            },
            fetched_at_utc: now_utc,
            token: token.clone(),
            attempt,
        };
        let Some(jobs) = &self.jobs else {
            return RefreshOutcome::Disabled;
        };
        if jobs.try_send(job).is_err() {
            self.last_error = Some("weather worker request queue is unavailable".to_owned());
            return RefreshOutcome::InFlight;
        }
        self.in_flight = Some(InFlight {
            token,
            started: now_mono,
            fetched_at_utc: now_utc,
            attempt,
        });
        let _ = kind;
        RefreshOutcome::Started { attempt }
    }

    pub fn poll(&mut self, now_utc: i64, now_mono: Instant) {
        while let Ok(completion) = self.completions.try_recv() {
            self.complete(completion, now_utc);
        }
        if let Some(in_flight) = &self.in_flight {
            if now_mono.duration_since(in_flight.started) >= self.config.request_deadline {
                in_flight.token.cancel();
                self.last_error = Some("weather request deadline expired".to_owned());
                // Keep the in-flight marker until the worker acknowledges the
                // cancellation. This prevents a second request overlapping a
                // provider that has not returned yet.
            }
        }
        if self.in_flight.is_none()
            && self.next_allowed_utc.is_some_and(|next| now_utc >= next)
            && self.retry_attempt > 0
        {
            let _ = self.request_refresh(now_utc, now_mono, RefreshKind::Poll);
        }
    }

    fn complete(&mut self, completion: FetchCompletion, now_utc: i64) {
        let Some(in_flight) = &self.in_flight else {
            return;
        };
        if !Arc::ptr_eq(&in_flight.token.0, &completion.token.0) {
            return;
        }
        self.in_flight = None;
        match completion.result {
            Ok(payload) => match WeatherReading::from_provider(
                payload,
                completion.fetched_at_utc,
                self.config.source.clone(),
            ) {
                Ok(reading) => match &mut self.cache {
                    Some(cache) => match cache.replace(reading, now_utc) {
                        Ok(()) => self.persist(now_utc),
                        Err(error_value) => self.fail(error_value),
                    },
                    None => match WeatherCache::new(reading, now_utc) {
                        Ok(cache) => {
                            self.cache = Some(cache);
                            self.persist(now_utc);
                        }
                        Err(error_value) => self.fail(error_value),
                    },
                },
                Err(error_value) => self.fail(error_value),
            },
            Err(provider_error) => self.fail(provider_error),
        }
        if self.last_error.is_none() {
            self.retry_attempt = 0;
            self.next_allowed_utc =
                Some(completion.fetched_at_utc.saturating_add(
                    self.config.poll_interval.as_secs().min(i64::MAX as u64) as i64,
                ));
            self.cache_epoch = self.cache_epoch.saturating_add(1);
        } else if completion.attempt <= self.config.retry_backoff.len() as u8 {
            let delay = self.config.retry_backoff[usize::from(completion.attempt - 1)];
            self.retry_attempt = completion.attempt;
            self.next_allowed_utc = Some(now_utc.saturating_add(delay.as_secs() as i64));
        } else {
            self.retry_attempt = 0;
            self.next_allowed_utc =
                Some(now_utc.saturating_add(self.config.poll_interval.as_secs() as i64));
        }
    }

    fn persist(&mut self, now_utc: i64) {
        if let (Some(store), Some(cache)) = (&self.cache_store, &self.cache)
            && let Err(error_value) = store.save(cache, now_utc)
        {
            self.last_error = Some(error_value.to_string());
            return;
        }
        self.last_error = None;
    }

    fn fail(&mut self, error_value: impl fmt::Display) {
        self.last_error = Some(error_value.to_string());
    }

    pub fn snapshot(&self, now_utc: i64) -> WeatherSnapshot {
        let freshness = self.cache.as_ref().map_or(Freshness::Unknown, |cache| {
            cache.freshness(now_utc, self.config.stale_after)
        });
        let state = if !self.config.enabled {
            WeatherState::Disabled
        } else if self.last_error.is_some() {
            WeatherState::Error
        } else {
            match freshness {
                Freshness::Fresh => WeatherState::Fresh,
                Freshness::Stale => WeatherState::Stale,
                Freshness::Unknown => WeatherState::Unknown,
            }
        };
        WeatherSnapshot {
            reading: self.cache.as_ref().map(|cache| cache.reading.clone()),
            freshness,
            state,
            last_error: self.last_error.clone(),
        }
    }

    pub fn diagnostics(&self) -> WeatherDiagnostics {
        WeatherDiagnostics {
            in_flight: self.in_flight.is_some(),
            cache_epoch: self.cache_epoch,
            retry_attempt: self.retry_attempt,
        }
    }

    pub fn cache_epoch(&self) -> u64 {
        self.cache_epoch
    }
}

impl Drop for WeatherService {
    fn drop(&mut self) {
        if let Some(in_flight) = &self.in_flight {
            in_flight.token.cancel();
        }
        // Dropping the sender closes the worker's receive loop once the
        // provider returns. We intentionally do not join a possibly blocked
        // provider on the Wayland/event thread.
        self.jobs.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };

    fn location() -> Location {
        Location::new(45.0, -74.0).unwrap()
    }

    fn payload(condition: ProviderCondition) -> ProviderPayload {
        ProviderPayload {
            condition,
            temperature_c: Some(12.5),
            precipitation: matches!(condition, ProviderCondition::Rain),
            observed_at_utc: 0,
        }
    }

    fn reading() -> WeatherReading {
        WeatherReading::from_provider(payload(ProviderCondition::Rain), 1_000, "fake").unwrap()
    }

    #[derive(Clone)]
    struct FakeProvider {
        calls: Arc<Mutex<u32>>,
        response: Result<ProviderPayload, ProviderError>,
    }

    impl WeatherProvider for FakeProvider {
        fn fetch(
            &self,
            _request: ProviderRequest,
            _cancellation: &CancellationToken,
        ) -> Result<ProviderPayload, ProviderError> {
            *self.calls.lock().unwrap() += 1;
            self.response.clone()
        }
    }

    #[derive(Clone)]
    struct CancellableSlowProvider {
        calls: Arc<Mutex<u32>>,
        delay: Duration,
    }

    impl WeatherProvider for CancellableSlowProvider {
        fn fetch(
            &self,
            _request: ProviderRequest,
            cancellation: &CancellationToken,
        ) -> Result<ProviderPayload, ProviderError> {
            *self.calls.lock().unwrap() += 1;
            let started = Instant::now();
            while started.elapsed() < self.delay {
                if cancellation.is_cancelled() {
                    return Err(provider_error(
                        ProviderErrorKind::Cancelled,
                        "fake offline request cancelled",
                    ));
                }
                thread::sleep(Duration::from_millis(1));
            }
            Ok(payload(ProviderCondition::Clear))
        }
    }

    fn local_http_response(status: u16, delay: Duration) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            if !delay.is_zero() {
                thread::sleep(delay);
            }
            let response =
                format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = stream.write_all(response.as_bytes());
        });
        (format!("http://{address}/v1/forecast"), handle)
    }

    fn wait_for_completion(service: &mut WeatherService, now: i64) {
        for _ in 0..50 {
            service.poll(now, Instant::now());
            if !service.diagnostics().in_flight {
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("fake weather provider did not complete");
    }

    #[test]
    fn disabled_weather_makes_zero_provider_requests() {
        let calls = Arc::new(Mutex::new(0));
        let provider = FakeProvider {
            calls: calls.clone(),
            response: Ok(payload(ProviderCondition::Clear)),
        };
        let mut service = WeatherService::new(WeatherConfig::disabled(), provider).unwrap();
        assert_eq!(
            service.request_refresh(0, Instant::now(), RefreshKind::Startup),
            RefreshOutcome::Disabled
        );
        assert_eq!(*calls.lock().unwrap(), 0);
        assert_eq!(service.snapshot(0).state, WeatherState::Disabled);
    }

    #[test]
    fn provider_payload_is_normalized_and_rejects_future_or_invalid_values() {
        let normalized =
            WeatherReading::from_provider(payload(ProviderCondition::Rain), 1_000, "fake").unwrap();
        assert_eq!(normalized.condition, Weather::Rain);
        assert!(normalized.precipitation);
        assert!(
            WeatherReading::from_provider(
                ProviderPayload {
                    temperature_c: Some(f32::NAN),
                    ..payload(ProviderCondition::Clear)
                },
                1_000,
                "fake"
            )
            .is_err()
        );
        assert!(
            WeatherReading::from_provider(
                ProviderPayload {
                    observed_at_utc: 1_601,
                    ..payload(ProviderCondition::Clear)
                },
                1_000,
                "fake"
            )
            .is_err()
        );
    }

    #[test]
    fn open_meteo_current_response_maps_documented_wmo_values() {
        let response = r#"{
            "current": {
                "time": 1720000000,
                "temperature_2m": 18.25,
                "precipitation": 0.4,
                "weather_code": 63
            },
            "current_units": {
                "temperature_2m": "°C",
                "precipitation": "mm",
                "weather_code": "wmo code"
            }
        }"#;
        let payload = parse_open_meteo_response(response).unwrap();
        assert_eq!(payload.condition, ProviderCondition::Rain);
        assert_eq!(payload.temperature_c, Some(18.25));
        assert!(payload.precipitation);
        assert_eq!(payload.observed_at_utc, 1_720_000_000);
        assert!(parse_open_meteo_response(r#"{"current": {"weather_code": 0}}"#).is_err());
        assert!(OpenMeteoProvider::new(Duration::from_secs(1)).is_ok());
        assert!(OpenMeteoProvider::endpoint().starts_with("https://"));
    }

    #[test]
    fn open_meteo_maps_http_errors_and_request_timeout() {
        assert_eq!(classify_http_status(401), ProviderErrorKind::Authorization);
        assert_eq!(classify_http_status(429), ProviderErrorKind::RateLimited);
        assert_eq!(classify_http_status(500), ProviderErrorKind::Network);

        let (endpoint, server) = local_http_response(429, Duration::ZERO);
        let provider = OpenMeteoProvider::with_endpoint(Duration::from_secs(1), endpoint).unwrap();
        let result = provider.fetch(
            ProviderRequest {
                location: location(),
                requested_at_utc: 1_000,
            },
            &CancellationToken::default(),
        );
        assert_eq!(result.unwrap_err().kind, ProviderErrorKind::RateLimited);
        server.join().unwrap();

        let (endpoint, server) = local_http_response(200, Duration::from_millis(100));
        let provider =
            OpenMeteoProvider::with_endpoint(Duration::from_millis(10), endpoint).unwrap();
        let result = provider.fetch(
            ProviderRequest {
                location: location(),
                requested_at_utc: 1_000,
            },
            &CancellationToken::default(),
        );
        assert_eq!(result.unwrap_err().kind, ProviderErrorKind::Timeout);
        server.join().unwrap();
    }

    #[test]
    fn cache_round_trip_is_versioned_bounded_and_rejects_corruption() {
        let root =
            std::env::temp_dir().join(format!("worldwall-weather-cache-{}", unique_suffix()));
        let store = CacheStore::new(root.join("nested/weather.cache"));
        let cache = WeatherCache::new(reading(), 1_000).unwrap();
        store.save(&cache, 1_000).unwrap();
        assert_eq!(store.load(1_000).unwrap().unwrap(), cache);
        std::fs::write(store.path(), "schema_version=999\n").unwrap();
        assert!(store.load(1_000).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cache_directory_and_file_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("worldwall-weather-mode-{}", unique_suffix()));
        let store = CacheStore::new(root.join("weather.cache"));
        store
            .save(&WeatherCache::new(reading(), 1_000).unwrap(), 1_000)
            .unwrap();
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(store.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_cache_does_not_displace_valid_memory() {
        let root =
            std::env::temp_dir().join(format!("worldwall-weather-memory-{}", unique_suffix()));
        let store = CacheStore::new(root.join("weather.cache"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(store.path(), "corrupt\n").unwrap();
        let provider = FakeProvider {
            calls: Arc::new(Mutex::new(0)),
            response: Ok(payload(ProviderCondition::Clear)),
        };
        let mut service = WeatherService::new(
            WeatherConfig::enabled(location(), "fake").unwrap(),
            provider,
        )
        .unwrap();
        service.install_cache(WeatherCache::new(reading(), 1_000).unwrap());
        service.attach_cache_best_effort(store, 1_000).unwrap();
        assert_eq!(service.snapshot(1_000).weather(), Weather::Rain);
        assert_eq!(service.snapshot(1_000).state, WeatherState::Error);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_cache_is_replaced_after_a_valid_result() {
        let root =
            std::env::temp_dir().join(format!("worldwall-weather-recovery-{}", unique_suffix()));
        let store = CacheStore::new(root.join("weather.cache"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(store.path(), "not a weather cache\n").unwrap();

        let calls = Arc::new(Mutex::new(0));
        let provider = FakeProvider {
            calls: calls.clone(),
            response: Ok(payload(ProviderCondition::Clear)),
        };
        let mut service = WeatherService::new(
            WeatherConfig::enabled(location(), "fake").unwrap(),
            provider,
        )
        .unwrap();
        service
            .attach_cache_best_effort(store.clone(), 1_000)
            .unwrap();
        assert_eq!(service.snapshot(1_000).state, WeatherState::Error);
        assert!(service.snapshot(1_000).reading.is_none());

        assert!(matches!(
            service.request_refresh(1_000, Instant::now(), RefreshKind::Startup),
            RefreshOutcome::Started { .. }
        ));
        wait_for_completion(&mut service, 1_000);
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(service.snapshot(1_000).state, WeatherState::Fresh);
        assert_eq!(
            store.load(1_000).unwrap().unwrap().reading().condition,
            Weather::Clear
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn offline_request_timeout_cancels_without_overlapping_a_retry() {
        let calls = Arc::new(Mutex::new(0));
        let provider = CancellableSlowProvider {
            calls: calls.clone(),
            delay: Duration::from_millis(100),
        };
        let mut config = WeatherConfig::enabled(location(), "fake").unwrap();
        config.request_deadline = Duration::from_millis(5);
        config.retry_backoff = vec![Duration::from_secs(5)];
        let mut service = WeatherService::new(config, provider).unwrap();
        let start = Instant::now();
        assert!(matches!(
            service.request_refresh(1_000, start, RefreshKind::Startup),
            RefreshOutcome::Started { .. }
        ));
        thread::sleep(Duration::from_millis(10));
        service.poll(1_000, start + Duration::from_millis(10));
        assert!(service.diagnostics().in_flight);
        assert_eq!(
            service.request_refresh(1_000, Instant::now(), RefreshKind::Poll),
            RefreshOutcome::InFlight
        );
        wait_for_completion(&mut service, 1_000);
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(service.snapshot(1_000).state, WeatherState::Error);
        assert_eq!(service.diagnostics().retry_attempt, 1);
    }

    #[test]
    fn duplicate_resume_signals_coalesce_to_one_provider_request() {
        let calls = Arc::new(Mutex::new(0));
        let provider = FakeProvider {
            calls: calls.clone(),
            response: Ok(payload(ProviderCondition::Clear)),
        };
        let mut service = WeatherService::new(
            WeatherConfig::enabled(location(), "fake").unwrap(),
            provider,
        )
        .unwrap();
        let start = Instant::now();
        assert!(matches!(
            service.request_refresh(1_000, start, RefreshKind::Resume),
            RefreshOutcome::Started { .. }
        ));
        assert_eq!(
            service.request_refresh(1_001, start, RefreshKind::Resume),
            RefreshOutcome::InFlight
        );
        assert_eq!(
            service.request_refresh(1_002, start, RefreshKind::Resume),
            RefreshOutcome::InFlight
        );
        wait_for_completion(&mut service, 1_000);
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(service.cache_epoch(), 1);
    }

    #[test]
    fn cadence_and_one_in_flight_are_enforced() {
        let calls = Arc::new(Mutex::new(0));
        let provider = FakeProvider {
            calls: calls.clone(),
            response: Ok(payload(ProviderCondition::Clear)),
        };
        let config = WeatherConfig::enabled(location(), "fake").unwrap();
        let mut service = WeatherService::new(config, provider).unwrap();
        let start = Instant::now();
        assert_eq!(
            service.request_refresh(0, start, RefreshKind::Startup),
            RefreshOutcome::Started { attempt: 1 }
        );
        assert_eq!(
            service.request_refresh(1, start, RefreshKind::Poll),
            RefreshOutcome::InFlight
        );
        wait_for_completion(&mut service, 1_000);
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(service.snapshot(1_000).state, WeatherState::Fresh);
        assert_eq!(
            service.request_refresh(899, start, RefreshKind::Poll),
            RefreshOutcome::RateLimited
        );
        assert_eq!(
            service.request_refresh(900, start, RefreshKind::Poll),
            RefreshOutcome::Started { attempt: 1 }
        );
        wait_for_completion(&mut service, 1_000);
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    #[test]
    fn provider_error_keeps_last_valid_cache_and_retries_boundedly() {
        let calls = Arc::new(Mutex::new(0));
        let provider = FakeProvider {
            calls: calls.clone(),
            response: Err(ProviderError {
                kind: ProviderErrorKind::RateLimited,
                message: "rate limited".to_owned(),
            }),
        };
        let mut service = WeatherService::new(
            WeatherConfig::enabled(location(), "fake").unwrap(),
            provider,
        )
        .unwrap();
        service.install_cache(WeatherCache::new(reading(), 1_000).unwrap());
        let start = Instant::now();
        assert!(matches!(
            service.request_refresh(1_000, start, RefreshKind::Manual),
            RefreshOutcome::Started { .. }
        ));
        wait_for_completion(&mut service, 1_000);
        let snapshot = service.snapshot(1_000);
        assert_eq!(snapshot.reading.unwrap().condition, Weather::Rain);
        assert_eq!(snapshot.state, WeatherState::Error);
        assert_eq!(service.diagnostics().retry_attempt, 1);
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    #[test]
    fn late_completion_updates_only_cache_epoch_not_selection() {
        let calls = Arc::new(Mutex::new(0));
        let provider = FakeProvider {
            calls,
            response: Ok(payload(ProviderCondition::Snow)),
        };
        let mut service = WeatherService::new(
            WeatherConfig::enabled(location(), "fake").unwrap(),
            provider,
        )
        .unwrap();
        let start = Instant::now();
        assert!(matches!(
            service.request_refresh(2_000, start, RefreshKind::Poll),
            RefreshOutcome::Started { .. }
        ));
        wait_for_completion(&mut service, 2_000);
        assert_eq!(service.cache_epoch(), 1);
        // The service has no selector handle; a completion can only expose a
        // cache epoch/snapshot. Scene-selection generation remains owned by
        // SelectionController and is therefore unchanged by this path.
        assert_eq!(service.diagnostics().cache_epoch, 1);
        assert_eq!(service.snapshot(2_000).weather(), Weather::Snow);
    }
}
