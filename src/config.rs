//! Versioned, transactional runtime configuration.
//!
//! Parsing and validation happen on a complete candidate before the caller
//! replaces its active copy. Registry validation loads every listed package,
//! including the declared fallback, so a future selection cannot be pointed at
//! an incomplete registry by a successful config reload. Config errors never
//! echo coordinates or other location values.

use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use chrono_tz::Tz;

use crate::{
    environment::{Location, UtcOffsetSchedule},
    scene_pools::{LibraryCatalog, PoolSource, ScenePools},
    scene_registry::{SceneManifest, SceneRegistry},
    scheduler::Fps,
};

pub const CONFIG_SCHEMA_VERSION: u32 = 5;
const MIN_CONFIG_SCHEMA_VERSION: u32 = 1;
const MAX_CONFIG_BYTES: usize = 64 * 1024;
pub const DEFAULT_ANIMATION_SPEED_MULTIPLIER: f32 = 0.5;
const MIN_ANIMATION_SPEED_MULTIPLIER: f64 = 0.0;
const MAX_ANIMATION_SPEED_MULTIPLIER: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ConfigError {}

fn error(message: impl Into<String>) -> ConfigError {
    ConfigError(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfiguredTimeZone {
    FixedOffsetMinutes(i32),
    Iana(Tz),
}

impl ConfiguredTimeZone {
    pub fn to_schedule(self) -> Result<UtcOffsetSchedule, ConfigError> {
        match self {
            Self::FixedOffsetMinutes(minutes) => UtcOffsetSchedule::fixed(minutes)
                .map_err(|_| error("UTC offset must be between -14:00 and +14:00")),
            Self::Iana(timezone) => Ok(UtcOffsetSchedule::iana(timezone)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherProvider {
    Disabled,
    OpenMeteo,
}

#[derive(Debug)]
pub struct PreparedAppConfig {
    pub config: AppConfig,
    pub registry: Option<SceneRegistry>,
    /// Validated schema-v5 pool/rule state, present only when
    /// `scene_pools_file` is configured.
    pub pools: Option<ScenePools>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub schema_version: u32,
    pub registry_dir: Option<PathBuf>,
    pub fps: Fps,
    /// Global multiplier for every palette animation effect; 1.0 is authored speed.
    pub animation_speed_multiplier: f32,
    /// Location for local civil-time/astronomy calculations.
    pub location: Option<Location>,
    /// Explicit civil-time zone; absent inputs intentionally yield unknown time.
    pub timezone: Option<ConfiguredTimeZone>,
    pub weather_provider: WeatherProvider,
    /// Opt-in same-family weather variant and temperature-family policy.
    pub adaptive_scenes: bool,
    /// Separate only for legacy environment-variable compatibility. File
    /// configuration always reuses `location` for the weather provider.
    pub weather_location: Option<Location>,
    /// Privacy-safe cache generation derived from config metadata, not its
    /// location values. A changed file cannot reuse a prior weather snapshot.
    pub cache_scope: Option<u128>,
    pub source_path: Option<PathBuf>,
    /// Optional schema-v5 reference to the separately versioned pool/rules
    /// sidecar. Absence preserves legacy registry selection behavior.
    pub scene_pools_file: Option<PathBuf>,
}

impl AppConfig {
    pub fn from_environment() -> Result<Self, ConfigError> {
        if let Some(path) = config_path_from_environment() {
            // An explicitly selected file is authoritative. Do not mix in
            // privacy-sensitive values or overrides from ambient variables.
            return Self::load(path);
        }

        let fps = match std::env::var("WORLDWALL_FPS").ok().as_deref() {
            Some("15") => Fps::Fifteen,
            Some("30") | None => Fps::Thirty,
            Some(value) => {
                return Err(error(format!(
                    "WORLDWALL_FPS must be 15 or 30, got {value:?}"
                )));
            }
        };
        let registry_dir = std::env::var_os("WORLDWALL_REGISTRY_DIR").map(PathBuf::from);
        let location = legacy_location(
            "WORLDWALL_LOCATION_LATITUDE",
            "WORLDWALL_LOCATION_LONGITUDE",
        )?;
        let timezone = match std::env::var("WORLDWALL_UTC_OFFSET_MINUTES") {
            Ok(value) => {
                let minutes = value
                    .parse::<i32>()
                    .map_err(|_| error("WORLDWALL_UTC_OFFSET_MINUTES must be a whole number"))?;
                validate_offset(minutes)?;
                Some(ConfiguredTimeZone::FixedOffsetMinutes(minutes))
            }
            Err(std::env::VarError::NotPresent) => None,
            Err(_) => return Err(error("reading WORLDWALL_UTC_OFFSET_MINUTES failed")),
        };
        let (weather_provider, weather_location) = legacy_weather()?;
        let config = Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            registry_dir,
            fps,
            animation_speed_multiplier: DEFAULT_ANIMATION_SPEED_MULTIPLIER,
            location,
            timezone,
            weather_provider,
            adaptive_scenes: false,
            weather_location,
            cache_scope: None,
            source_path: None,
            scene_pools_file: None,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        Ok(Self::load_prepared(path)?.config)
    }

    /// Parse a complete config and validate its registry, sidecar, and pool
    /// packages on the caller's worker thread. The returned registry/pool pair
    /// is the exact validated candidate to commit atomically; callers must not
    /// reload it on the Wayland owner.
    pub fn load_prepared(path: impl AsRef<Path>) -> Result<PreparedAppConfig, ConfigError> {
        let config = Self::parse_file(path.as_ref())?;
        let (registry, pools) = config.prepare_sources()?;
        Ok(PreparedAppConfig {
            config,
            registry,
            pools,
        })
    }

    /// Validate the registry, the optional sidecar, and every referenced pool
    /// package as one candidate, and build the selector's complete scene set.
    pub fn prepare_sources(
        &self,
    ) -> Result<(Option<SceneRegistry>, Option<ScenePools>), ConfigError> {
        let mut registry = self.load_validated_registry()?;
        let Some(rules_path) = self.scene_pools_file.as_ref() else {
            return Ok((registry, None));
        };
        let pools = ScenePools::load(rules_path).map_err(|pools_error| {
            ConfigError(format!("pool rules validation failed: {pools_error}"))
        })?;

        // Registry-sourced pools must name packages of the configured
        // registry; library-sourced pools resolve through the local catalog.
        for pool in pools.pools() {
            for scene_id in &pool.scene_ids {
                match pool.source {
                    PoolSource::Registry => {
                        if registry
                            .as_ref()
                            .and_then(|registry| registry.get(scene_id))
                            .is_none()
                        {
                            return Err(error(format!(
                                "pool {scene_id:?} is not a package of the configured registry"
                            )));
                        }
                    }
                    PoolSource::Library => {}
                }
            }
        }
        let library = Self::library_root_for(rules_path);
        let mut library_manifests = Vec::new();
        if pools.pool_scene_ids(PoolSource::Library).next().is_some() {
            let catalog = LibraryCatalog::load(&library).map_err(|catalog_error| {
                ConfigError(format!("pool library validation failed: {catalog_error}"))
            })?;
            for scene_id in pools.pool_scene_ids(PoolSource::Library) {
                let Some(entry) = catalog.get(scene_id) else {
                    return Err(error(format!(
                        "pool scene {scene_id:?} is not in the library catalog"
                    )));
                };
                let (manifest, _) = SceneManifest::load(entry.package.join("scene.toml")).map_err(
                    |registry_error| {
                        ConfigError(format!(
                            "pool package {scene_id:?} validation failed: {registry_error}"
                        ))
                    },
                )?;
                if manifest.id != entry.id {
                    return Err(error(format!(
                        "pool package {scene_id:?} declares a different scene ID"
                    )));
                }
                library_manifests.push(manifest);
            }
        }
        if !library_manifests.is_empty() {
            let registry = registry.get_or_insert_with(SceneRegistry::empty);
            // In adaptive mode library entries stay manual-only: they must
            // never become CR-0006 families and registry validation is
            // unchanged. Non-adaptive pool mode selects from the union of
            // pool sources, narrowed by the shared eligibility gate.
            registry
                .add_pool_packages(library_manifests, !self.adaptive_scenes)
                .map_err(|registry_error| {
                    ConfigError(format!("pool package conflict: {registry_error}"))
                })?;
        }
        // A configured sidecar owns selection semantics even when all pools
        // are empty or no registry/library package is available. Keep an
        // empty source so startup fails closed instead of taking the legacy
        // synthetic-scene path outside the eligibility filter.
        let registry = Some(registry.unwrap_or_else(SceneRegistry::empty));
        Ok((registry, Some(pools)))
    }

    /// Library-sourced pools resolve through the library catalog that sits
    /// next to the configured sidecar, falling back to the curator's XDG
    /// default library root. No implicit package discovery happens here.
    fn library_root_for(rules_path: &Path) -> PathBuf {
        if let Some(parent) = rules_path.parent()
            && parent.join("library.toml").is_file()
        {
            return parent.to_path_buf();
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home.as_ref().map(|home| home.join(".local/share")));
        data_home
            .unwrap_or_else(|| PathBuf::from(".local/share"))
            .join("worldwall/scene-library")
    }

    fn parse_file(path: &Path) -> Result<Self, ConfigError> {
        let metadata = fs::symlink_metadata(path).map_err(|io_error| {
            error(format!(
                "config {} is unavailable: {io_error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(error("config path must be a regular non-symlink file"));
        }
        if metadata.len() > MAX_CONFIG_BYTES as u64 {
            return Err(error("config exceeds the bounded size"));
        }
        let bytes = fs::read(path)
            .map_err(|io_error| error(format!("reading config failed: {io_error}")))?;
        let text = String::from_utf8(bytes).map_err(|_| error("config must be UTF-8"))?;
        let value: toml::Value = toml::from_str(&text).map_err(|parse_error| {
            let line = parse_error.span().map(|span| {
                text[..span.start.min(text.len())]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1
            });
            line.map_or_else(
                || error("config TOML is malformed"),
                |line| error(format!("config TOML is malformed near line {line}")),
            )
        })?;
        let table = value
            .as_table()
            .ok_or_else(|| error("config root must be a TOML table"))?;
        reject_unknown_keys(
            table,
            &[
                "schema_version",
                "fps",
                "animation_speed_multiplier",
                "registry_dir",
                "scene_pools_file",
                "location",
                "weather",
            ],
            "config",
        )?;

        let schema_version = required_u32(table, "schema_version", "config")?;
        if !(MIN_CONFIG_SCHEMA_VERSION..=CONFIG_SCHEMA_VERSION).contains(&schema_version) {
            return Err(error(format!(
                "unsupported config schema_version {schema_version}; expected 1 through {CONFIG_SCHEMA_VERSION}"
            )));
        }
        let fps = match required_integer(table, "fps", "config")? {
            15 => Fps::Fifteen,
            30 => Fps::Thirty,
            _ => return Err(error("config fps must be 15 or 30")),
        };
        let animation_speed_multiplier = match table.get("animation_speed_multiplier") {
            None => DEFAULT_ANIMATION_SPEED_MULTIPLIER,
            Some(_) if schema_version < 4 => {
                return Err(error(
                    "config animation_speed_multiplier requires schema_version 4",
                ));
            }
            Some(_) => {
                let multiplier = required_number(table, "animation_speed_multiplier", "config")?;
                if !multiplier.is_finite()
                    || !(MIN_ANIMATION_SPEED_MULTIPLIER..=MAX_ANIMATION_SPEED_MULTIPLIER)
                        .contains(&multiplier)
                {
                    return Err(error(
                        "config animation_speed_multiplier must be between 0.0 and 1.0",
                    ));
                }
                multiplier as f32
            }
        };
        let registry = required_string(table, "registry_dir", "config")?;
        let registry_dir = (registry != "none").then(|| PathBuf::from(registry));
        let scene_pools_file = match table.get("scene_pools_file") {
            None => None,
            Some(_) if schema_version < 5 => {
                return Err(error("config scene_pools_file requires schema_version 5"));
            }
            Some(_) => {
                let path = required_string(table, "scene_pools_file", "config")?;
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    return Err(error("config scene_pools_file must be an absolute path"));
                }
                Some(path)
            }
        };

        let (location, timezone, weather_provider, adaptive_scenes, weather_location) =
            if schema_version == 1 {
                if table.contains_key("location") || table.contains_key("weather") {
                    return Err(error(
                        "config schema_version 1 does not support location or weather tables",
                    ));
                }
                (None, None, WeatherProvider::Disabled, false, None)
            } else {
                let (location, timezone) = match table.get("location") {
                    None => (None, None),
                    Some(value) => {
                        let location_table = value
                            .as_table()
                            .ok_or_else(|| error("config [location] must be a TOML table"))?;
                        reject_unknown_keys(
                            location_table,
                            &["latitude", "longitude", "timezone"],
                            "config [location]",
                        )?;
                        let latitude =
                            required_number(location_table, "latitude", "config [location]")?;
                        let longitude =
                            required_number(location_table, "longitude", "config [location]")?;
                        let location = Location::new(latitude, longitude)
                            .map_err(|_| error("config [location] coordinates are invalid"))?;
                        let timezone_name =
                            required_string(location_table, "timezone", "config [location]")?;
                        let timezone = timezone_name.parse::<Tz>().map_err(|_| {
                            error("config [location] timezone is not a recognized IANA name")
                        })?;
                        (Some(location), Some(ConfiguredTimeZone::Iana(timezone)))
                    }
                };
                let provider = match table.get("weather") {
                    None => WeatherProvider::Disabled,
                    Some(value) => {
                        let weather_table = value
                            .as_table()
                            .ok_or_else(|| error("config [weather] must be a TOML table"))?;
                        let allowed_keys: &[&str] = if schema_version >= 3 {
                            &["provider", "adaptive_scenes"]
                        } else {
                            &["provider"]
                        };
                        reject_unknown_keys(weather_table, allowed_keys, "config [weather]")?;
                        match required_string(weather_table, "provider", "config [weather]")? {
                            "disabled" => WeatherProvider::Disabled,
                            "open-meteo" => WeatherProvider::OpenMeteo,
                            _ => {
                                return Err(error(
                                    "config [weather] provider must be disabled or open-meteo",
                                ));
                            }
                        }
                    }
                };
                let adaptive_scenes = if schema_version >= 3 {
                    match table.get("weather").and_then(toml::Value::as_table) {
                        Some(weather_table) => weather_table
                            .get("adaptive_scenes")
                            .map(|value| {
                                value.as_bool().ok_or_else(|| {
                                    error("config [weather] adaptive_scenes must be a Boolean")
                                })
                            })
                            .transpose()?
                            .unwrap_or(false),
                        None => false,
                    }
                } else {
                    false
                };
                if provider == WeatherProvider::OpenMeteo && location.is_none() {
                    return Err(error(
                        "config [weather] open-meteo requires a complete [location] table",
                    ));
                }
                let weather_location = (provider == WeatherProvider::OpenMeteo)
                    .then_some(location)
                    .flatten();
                (
                    location,
                    timezone,
                    provider,
                    adaptive_scenes,
                    weather_location,
                )
            };

        let config = Self {
            schema_version,
            registry_dir,
            fps,
            animation_speed_multiplier,
            location,
            timezone,
            weather_provider,
            adaptive_scenes,
            weather_location,
            cache_scope: cache_scope_from_metadata(&metadata),
            source_path: Some(path.to_path_buf()),
            scene_pools_file,
        };
        config.validate_fields()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.validate_fields()?;
        self.load_validated_registry().map(drop)
    }

    fn validate_fields(&self) -> Result<(), ConfigError> {
        if !(MIN_CONFIG_SCHEMA_VERSION..=CONFIG_SCHEMA_VERSION).contains(&self.schema_version) {
            return Err(error(format!(
                "unsupported config schema_version {}; expected 1 through {CONFIG_SCHEMA_VERSION}",
                self.schema_version
            )));
        }
        if !self.animation_speed_multiplier.is_finite()
            || !(MIN_ANIMATION_SPEED_MULTIPLIER as f32..=MAX_ANIMATION_SPEED_MULTIPLIER as f32)
                .contains(&self.animation_speed_multiplier)
        {
            return Err(error(
                "animation_speed_multiplier must be between 0.0 and 1.0",
            ));
        }
        if self.weather_provider == WeatherProvider::OpenMeteo && self.weather_location.is_none() {
            return Err(error("enabled weather requires an explicit location"));
        }
        if self.adaptive_scenes && self.weather_provider != WeatherProvider::OpenMeteo {
            return Err(error(
                "adaptive scene selection requires the Open-Meteo provider to be enabled",
            ));
        }
        Ok(())
    }

    fn load_validated_registry(&self) -> Result<Option<SceneRegistry>, ConfigError> {
        let registry = self
            .registry_dir
            .as_ref()
            .map(SceneRegistry::load)
            .transpose()
            .map_err(|error| ConfigError(format!("registry validation failed: {error}")))?;
        if self.adaptive_scenes {
            registry
                .as_ref()
                .ok_or_else(|| error("adaptive scene selection requires registry_dir"))?
                .validate_adaptive()
                .map_err(|error| {
                    ConfigError(format!("adaptive registry validation failed: {error}"))
                })?;
        }
        Ok(registry)
    }

    pub fn reload_path(&self) -> Result<PathBuf, ConfigError> {
        self.source_path.clone().ok_or_else(|| {
            error("reload-config requires WORLDWALL_CONFIG or an explicit config path")
        })
    }
}

fn reject_unknown_keys(
    table: &toml::map::Map<String, toml::Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), ConfigError> {
    if let Some(key) = table.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(error(format!("{context} contains unknown key {key:?}")));
    }
    Ok(())
}

fn required_u32(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    context: &str,
) -> Result<u32, ConfigError> {
    let value = required_integer(table, key, context)?;
    u32::try_from(value)
        .map_err(|_| error(format!("{context} {key} must be a nonnegative integer")))
}

fn required_integer(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    context: &str,
) -> Result<i64, ConfigError> {
    table
        .get(key)
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| error(format!("{context} {key} must be an integer")))
}

fn required_number(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    context: &str,
) -> Result<f64, ConfigError> {
    table
        .get(key)
        .and_then(|value| {
            value
                .as_float()
                .or_else(|| value.as_integer().map(|v| v as f64))
        })
        .ok_or_else(|| error(format!("{context} {key} must be a number")))
}

fn required_string<'a>(
    table: &'a toml::map::Map<String, toml::Value>,
    key: &str,
    context: &str,
) -> Result<&'a str, ConfigError> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| error(format!("{context} {key} must be a string")))
}

fn legacy_location(
    latitude_name: &str,
    longitude_name: &str,
) -> Result<Option<Location>, ConfigError> {
    let latitude = std::env::var(latitude_name);
    let longitude = std::env::var(longitude_name);
    match (latitude, longitude) {
        (Ok(latitude), Ok(longitude)) => {
            let latitude = latitude
                .parse::<f64>()
                .map_err(|_| error("legacy location latitude is invalid"))?;
            let longitude = longitude
                .parse::<f64>()
                .map_err(|_| error("legacy location longitude is invalid"))?;
            Location::new(latitude, longitude)
                .map(Some)
                .map_err(|_| error("legacy location coordinates are invalid"))
        }
        (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => Ok(None),
        _ => Err(error(
            "legacy location latitude and longitude must be set together",
        )),
    }
}

fn legacy_weather() -> Result<(WeatherProvider, Option<Location>), ConfigError> {
    let provider = match std::env::var("WORLDWALL_WEATHER") {
        Ok(value) if value == "open-meteo" => WeatherProvider::OpenMeteo,
        Ok(value) if value == "disabled" => WeatherProvider::Disabled,
        Ok(_) => return Err(error("WORLDWALL_WEATHER must be disabled or open-meteo")),
        Err(std::env::VarError::NotPresent) => WeatherProvider::Disabled,
        Err(_) => return Err(error("reading WORLDWALL_WEATHER failed")),
    };
    if provider == WeatherProvider::Disabled {
        return Ok((provider, None));
    }
    let location = legacy_location("WORLDWALL_WEATHER_LATITUDE", "WORLDWALL_WEATHER_LONGITUDE")?
        .ok_or_else(|| error("Open-Meteo requires explicit weather coordinates"))?;
    Ok((provider, Some(location)))
}

fn validate_offset(offset_minutes: i32) -> Result<(), ConfigError> {
    if (-14 * 60..=14 * 60).contains(&offset_minutes) {
        Ok(())
    } else {
        Err(error("UTC offset must be between -14:00 and +14:00"))
    }
}

fn cache_scope_from_metadata(metadata: &fs::Metadata) -> Option<u128> {
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let mut scope = modified ^ u128::from(metadata.len()).rotate_left(67);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        scope ^= u128::from(metadata.dev()).rotate_left(19);
        scope ^= u128::from(metadata.ino()).rotate_left(53);
    }
    Some(scope)
}

fn config_path_from_environment() -> Option<PathBuf> {
    std::env::var_os("WORLDWALL_CONFIG").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "worldwall-s10-config-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    // ------------------------------------------------------------------
    // S12-AC4: optional schema-v5 sidecar reference, one validated candidate.

    fn write_png(width: u32, height: u32, color: png::ColorType, bytes: &[u8]) -> Vec<u8> {
        use png::BitDepth;
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, width, height);
            encoder.set_color(color);
            encoder.set_depth(BitDepth::Eight);
            if color == png::ColorType::Indexed {
                encoder.set_palette(vec![0, 0, 0, 255, 255, 255]);
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(bytes).unwrap();
        }
        encoded
    }

    fn write_package(directory: &Path, id: &str) {
        fs::create_dir_all(directory).unwrap();
        fs::write(
            directory.join("scene.png"),
            write_png(2, 1, png::ColorType::Indexed, &[0, 1]),
        )
        .unwrap();
        let mut palette = vec![9u8; 256 * 4];
        for entry in palette.chunks_mut(4) {
            entry[3] = 255;
        }
        fs::write(
            directory.join("palette.png"),
            write_png(256, 1, png::ColorType::Rgba, &palette),
        )
        .unwrap();
        fs::write(
            directory.join("scene.toml"),
            format!(
                "schema_version = 1\nid = \"{id}\"\nimage = \"scene.png\"\npalette = \"palette.png\"\n"
            ),
        )
        .unwrap();
    }

    fn write_library(root: &Path) {
        let digest = "b".repeat(64);
        fs::create_dir_all(root).unwrap();
        write_package(&root.join("canvascycle-demo"), "canvascycle-demo");
        fs::write(
            root.join("library.toml"),
            format!(
                "schema_version = 1\n\n[[scene]]\nid = \"canvascycle-demo\"\npackage = \"canvascycle-demo\"\nsource_collection = \"canvascycle\"\nworld = \"unknown\"\nappearance = \"unknown\"\nimage = \"scene.png\"\nimage_sha256 = \"{digest}\"\npalette = \"palette.png\"\npalette_sha256 = \"{digest}\"\nsource_format = \"canvascycle-legacy-object-literal-v1\"\nsource_sha256 = \"{digest}\"\nsource_url = \"https://example.invalid/capture\"\nsource_page = \"https://example.invalid/\"\nsource_title = \"Demo\"\nlicense_state = \"unreviewed\"\n"
            ),
        )
        .unwrap();
    }

    fn write_registry(root: &Path) {
        fs::create_dir_all(root).unwrap();
        write_package(&root.join("village"), "village");
        fs::write(
            root.join("registry.toml"),
            "schema_version = 1\nscenes = [\"village\"]\nfallback = \"village\"\n",
        )
        .unwrap();
    }

    fn write_pools(path: &Path, document: &str) {
        fs::write(path, document).unwrap();
    }

    const LIBRARY_POOL: &str = "schema_version = 1\n\n[[pool]]\nid = \"canvascycle\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"canvascycle-demo\"]\n";

    #[test]
    fn schema_v5_sidecar_reference_is_opt_in_absolute_and_version_gated() {
        let root = path("schema5");
        fs::create_dir_all(&root).unwrap();
        write_pools(&root.join("scene-pools.toml"), LIBRARY_POOL);

        // Older schemas reject the key; relative references are refused.
        fs::write(
            root.join("v4.toml"),
            "schema_version = 4\nfps = 30\nregistry_dir = \"none\"\nscene_pools_file = \"scene-pools.toml\"\n",
        )
        .unwrap();
        assert!(
            AppConfig::load(root.join("v4.toml"))
                .unwrap_err()
                .to_string()
                .contains("requires schema_version 5")
        );
        fs::write(
            root.join("relative.toml"),
            "schema_version = 5\nfps = 30\nregistry_dir = \"none\"\nscene_pools_file = \"scene-pools.toml\"\n",
        )
        .unwrap();
        assert!(
            AppConfig::load(root.join("relative.toml"))
                .unwrap_err()
                .to_string()
                .contains("absolute path")
        );

        // Absence keeps schema-v5 readable with legacy behavior.
        fs::write(
            root.join("plain.toml"),
            "schema_version = 5\nfps = 30\nregistry_dir = \"none\"\n",
        )
        .unwrap();
        let prepared = AppConfig::load_prepared(root.join("plain.toml")).unwrap();
        assert!(prepared.pools.is_none(), "no sidecar means legacy behavior");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pool_candidate_validates_sidecar_packages_and_rules_as_one_unit() {
        let root = path("pools5");
        fs::create_dir_all(&root).unwrap();
        write_library(&root);
        write_registry(&root.join("registry"));
        let sidecar = root.join("scene-pools.toml");
        write_pools(&sidecar, LIBRARY_POOL);
        fs::write(
            root.join("config.toml"),
            format!(
                "schema_version = 5\nfps = 30\nregistry_dir = \"{}\"\nscene_pools_file = \"{}\"\n",
                root.join("registry").display(),
                sidecar.display()
            ),
        )
        .unwrap();

        // Library pool packages join the automatic candidate set and are
        // fully decoded before the candidate may commit.
        let prepared = AppConfig::load_prepared(root.join("config.toml")).unwrap();
        let registry = prepared.registry.unwrap();
        assert!(registry.get("canvascycle-demo").is_some());
        assert!(registry.get("village").is_some());
        assert!(prepared.pools.is_some());

        // A registry pool naming an unknown registry package fails the whole
        // candidate instead of committing partial state.
        write_pools(
            &sidecar,
            "schema_version = 1\n\n[[pool]]\nid = \"registry\"\nsource = \"registry\"\nenabled = true\nscene_ids = [\"missing-scene\"]\n",
        );
        assert!(
            AppConfig::load_prepared(root.join("config.toml"))
                .unwrap_err()
                .to_string()
                .contains("not a package of the configured registry")
        );

        // Library pools must resolve through the adjacent catalog.
        write_pools(
            &sidecar,
            "schema_version = 1\n\n[[pool]]\nid = \"canvascycle\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"missing-library-scene\"]\n",
        );
        assert!(
            AppConfig::load_prepared(root.join("config.toml"))
                .unwrap_err()
                .to_string()
                .contains("not in the library catalog")
        );

        // A malformed sidecar keeps the previous live config (the caller
        // applies only validated candidates).
        write_pools(&sidecar, "schema_version = 99\n");
        assert!(AppConfig::load_prepared(root.join("config.toml")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_pool_sidecar_without_registry_fails_closed_instead_of_defaulting_scene() {
        let root = path("empty-pools-no-registry");
        fs::create_dir_all(&root).unwrap();
        let sidecar = root.join("scene-pools.toml");
        write_pools(
            &sidecar,
            "schema_version = 1\n\n[[pool]]\nid = \"empty\"\nsource = \"library\"\nenabled = true\nscene_ids = []\n",
        );
        fs::write(
            root.join("config.toml"),
            format!(
                "schema_version = 5\nfps = 30\nregistry_dir = \"none\"\nscene_pools_file = \"{}\"\n",
                sidecar.display()
            ),
        )
        .unwrap();

        let prepared = AppConfig::load_prepared(root.join("config.toml")).unwrap();
        let registry = prepared.registry.expect(
            "pool mode must materialize an empty registry rather than permit the synthetic scene",
        );
        assert_eq!(registry.len(), 0);
        let mut selector = crate::scene_registry::SelectionController::new(registry);
        selector.set_pools(prepared.pools);
        assert!(matches!(
            selector.request(
                crate::scene_registry::EnvironmentSnapshot::unknown(),
                None,
                0,
                crate::scene_registry::SelectionTrigger::Startup,
            ),
            Err(crate::scene_registry::SelectionResult::NoScene(reason))
                if reason == "no eligible scene"
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn config_rejects_unknown_duplicate_and_invalid_values_atomically() {
        let path = path("invalid");
        fs::write(
            &path,
            "schema_version = 1\nfps = 60\nregistry_dir = \"none\"\n",
        )
        .unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::write(
            &path,
            "schema_version = 1\nfps = 30\nfps = 15\nregistry_dir = \"none\"\n",
        )
        .unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn schema_v1_config_remains_valid_and_preserves_future_selection_safety() {
        let path = path("v1-valid");
        fs::write(
            &path,
            "schema_version = 1\nfps = 15\nregistry_dir = \"none\"\n",
        )
        .unwrap();
        let config = AppConfig::load(&path).unwrap();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.registry_dir, None);
        assert_eq!(config.fps, Fps::Fifteen);
        assert_eq!(config.weather_provider, WeatherProvider::Disabled);
        assert!(!config.adaptive_scenes);
        assert_eq!(config.location, None);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn schema_v2_parses_explicit_iana_location_and_disabled_weather() {
        let path = path("v2-location");
        fs::write(
            &path,
            "schema_version = 2\nfps = 30\nregistry_dir = \"none\"\n[location]\nlatitude = 0.0\nlongitude = 0.0\ntimezone = \"America/Los_Angeles\"\n[weather]\nprovider = \"disabled\"\n",
        )
        .unwrap();
        let config = AppConfig::load(&path).unwrap();
        assert_eq!(config.location.unwrap().latitude_degrees(), 0.0);
        assert_eq!(config.weather_provider, WeatherProvider::Disabled);
        assert!(!config.adaptive_scenes);
        assert!(matches!(
            config.timezone,
            Some(ConfiguredTimeZone::Iana(Tz::America__Los_Angeles))
        ));
        assert_eq!(config.weather_location, None);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn schema_v2_open_meteo_requires_valid_location_and_reuses_it() {
        let path = path("v2-weather");
        fs::write(
            &path,
            "schema_version = 2\nfps = 30\nregistry_dir = \"none\"\n[weather]\nprovider = \"open-meteo\"\n",
        )
        .unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::write(
            &path,
            "schema_version = 2\nfps = 30\nregistry_dir = \"none\"\n[location]\nlatitude = 0.0\nlongitude = 0.0\ntimezone = \"America/Los_Angeles\"\n[weather]\nprovider = \"open-meteo\"\n",
        )
        .unwrap();
        let config = AppConfig::load(&path).unwrap();
        assert_eq!(config.weather_provider, WeatherProvider::OpenMeteo);
        assert!(!config.adaptive_scenes);
        assert_eq!(config.weather_location, config.location);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn schema_v4_animation_multiplier_defaults_globally_and_is_bounded() {
        let path = path("v4-animation-speed");
        let valid = "schema_version = 4\nfps = 30\nregistry_dir = \"none\"\nanimation_speed_multiplier = 0.25\n";
        fs::write(&path, valid).unwrap();
        let config = AppConfig::load(&path).unwrap();
        assert_eq!(config.animation_speed_multiplier, 0.25);

        for value in ["-0.1", "1.01", "nan", "inf"] {
            let invalid = format!(
                "schema_version = 4\nfps = 30\nregistry_dir = \"none\"\nanimation_speed_multiplier = {value}\n"
            );
            fs::write(&path, invalid).unwrap();
            assert!(AppConfig::load(&path).is_err(), "accepted {value}");
        }

        fs::write(
            &path,
            "schema_version = 3\nfps = 30\nregistry_dir = \"none\"\n[weather]\nprovider = \"disabled\"\n",
        )
        .unwrap();
        assert_eq!(
            AppConfig::load(&path).unwrap().animation_speed_multiplier,
            DEFAULT_ANIMATION_SPEED_MULTIPLIER
        );
        fs::write(
            &path,
            "schema_version = 3\nfps = 30\nregistry_dir = \"none\"\nanimation_speed_multiplier = 0.5\n",
        )
        .unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn schema_v3_adaptive_policy_is_opt_in_and_requires_enabled_weather_and_registry() {
        let path = path("v3-adaptive");
        let disabled = r#"schema_version = 3
fps = 30
registry_dir = "none"
[weather]
provider = "disabled"
adaptive_scenes = false
"#;
        std::fs::write(&path, disabled).unwrap();
        let config = AppConfig::load(&path).unwrap();
        assert!(!config.adaptive_scenes);

        let no_provider = disabled.replace("adaptive_scenes = false", "adaptive_scenes = true");
        std::fs::write(&path, no_provider).unwrap();
        assert!(
            AppConfig::load(&path)
                .unwrap_err()
                .to_string()
                .contains("requires the Open-Meteo provider")
        );

        let no_registry = r#"schema_version = 3
fps = 30
registry_dir = "none"
[location]
latitude = 1
longitude = 2
timezone = "Etc/UTC"
[weather]
provider = "open-meteo"
adaptive_scenes = true
"#;
        std::fs::write(&path, no_registry).unwrap();
        assert!(
            AppConfig::load(&path)
                .unwrap_err()
                .to_string()
                .contains("requires registry_dir")
        );

        let schema2 = no_registry.replace("schema_version = 3", "schema_version = 2");
        std::fs::write(&path, schema2).unwrap();
        assert!(AppConfig::load(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn schema_v2_rejects_unknown_fields_bad_zone_and_partial_location() {
        let path = path("v2-invalid");
        let valid_header = "schema_version = 2\nfps = 30\nregistry_dir = \"none\"\n";
        fs::write(
            &path,
            format!("{valid_header}[location]\nlatitude = 0\nlongitude = 0\n"),
        )
        .unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::write(
            &path,
            format!("{valid_header}[location]\nlatitude = 0\nlongitude = 0\ntimezone = \"Mars/Olympus\"\n"),
        )
        .unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::write(&path, format!("{valid_header}unexpected = true\n")).unwrap();
        assert!(AppConfig::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
