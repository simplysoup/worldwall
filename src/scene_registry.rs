//! Validated scene packages, deterministic selection, and transactional swaps.
//!
//! This module owns scene metadata and selection policy.  It does not know
//! about Wayland or wgpu: a complete `LoadedScene` is prepared before a caller
//! offers it to the renderer at a safe frame boundary.
//!
//! The public seam is intentionally implemented before the event loop consumes
//! it; the temporary allowance keeps the binary warning-clean during this
//! stage-local integration increment.
#![allow(dead_code)]

use calloop::channel::{self, Channel};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    fs::File,
    hash::{Hash, Hasher},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread,
};

use crate::{
    animation::Animation,
    scene::{self, Scene},
    scene_pools::{PoolEnvironment, PoolGate, ScenePools},
};

pub use crate::environment::{EnvironmentSnapshot, Season, TimeOfDay, Weather};

pub const SCENE_SCHEMA_VERSION: u32 = 2;
pub const REGISTRY_SCHEMA_VERSION: u32 = 1;
const MIN_SCENE_SCHEMA_VERSION: u32 = 1;
const MAX_REGISTRY_BYTES: usize = 1024 * 1024;
const MAX_ANIMATION_BYTES: usize = 1024 * 1024;
const TIME_WEIGHT: u32 = 5;
const WEATHER_WEIGHT: u32 = 4;
const SEASON_WEIGHT: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum TemperatureBand {
    Cold,
    Cool,
    Mild,
    Warm,
    Hot,
}

impl TemperatureBand {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cold => "cold",
            Self::Cool => "cool",
            Self::Mild => "mild",
            Self::Warm => "warm",
            Self::Hot => "hot",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "cold" => Self::Cold,
            "cool" => Self::Cool,
            "mild" => Self::Mild,
            "warm" => Self::Warm,
            "hot" => Self::Hot,
            _ => return None,
        })
    }

    pub fn classify(celsius: f32) -> Option<Self> {
        if !celsius.is_finite() || !(-100.0..=100.0).contains(&celsius) {
            return None;
        }
        Some(if celsius <= 0.0 {
            Self::Cold
        } else if celsius <= 10.0 {
            Self::Cool
        } else if celsius <= 20.0 {
            Self::Mild
        } else if celsius <= 28.0 {
            Self::Warm
        } else {
            Self::Hot
        })
    }

    /// Keep the prior band until temperature crosses its adjacent boundary by
    /// more than the approved 2°C hysteresis. Large jumps may cross multiple
    /// bands, but never skip the same boundary rule.
    pub fn with_hysteresis(celsius: f32, previous: Option<Self>) -> Option<Self> {
        if !celsius.is_finite() || !(-100.0..=100.0).contains(&celsius) {
            return None;
        }
        let Some(mut band) = previous else {
            return Self::classify(celsius);
        };
        loop {
            let next = match band {
                Self::Cold if celsius > 2.0 => Self::Cool,
                Self::Cool if celsius <= -2.0 => Self::Cold,
                Self::Cool if celsius > 12.0 => Self::Mild,
                Self::Mild if celsius <= 8.0 => Self::Cool,
                Self::Mild if celsius > 22.0 => Self::Warm,
                Self::Warm if celsius <= 18.0 => Self::Mild,
                Self::Warm if celsius > 30.0 => Self::Hot,
                Self::Hot if celsius <= 26.0 => Self::Warm,
                _ => return Some(band),
            };
            band = next;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecipitationAppearance {
    Clear,
    Cloudy,
    Rain,
    Snow,
    Unknown,
}

impl PrecipitationAppearance {
    pub fn from_weather(condition: Weather, precipitation: bool) -> Self {
        match condition {
            Weather::Clear => Self::Clear,
            Weather::Cloudy | Weather::Fog => Self::Cloudy,
            Weather::Rain => Self::Rain,
            Weather::Snow => Self::Snow,
            Weather::Storm if precipitation => Self::Rain,
            Weather::Storm | Weather::Unknown => Self::Unknown,
        }
    }

    fn preferences(self) -> &'static [Weather] {
        match self {
            Self::Clear => &[Weather::Clear],
            Self::Cloudy => &[Weather::Cloudy, Weather::Clear],
            Self::Rain => &[Weather::Rain, Weather::Cloudy, Weather::Clear],
            Self::Snow => &[Weather::Snow, Weather::Cloudy, Weather::Clear],
            Self::Unknown => &[Weather::Clear],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptiveSelectionState {
    pub family_id: String,
    pub temperature_band: TemperatureBand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptiveSelectionDecision {
    pub scene_id: String,
    pub state: AdaptiveSelectionState,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryError(String);

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for RegistryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneManifest {
    pub schema_version: u32,
    pub id: String,
    pub package_dir: PathBuf,
    pub image_path: PathBuf,
    pub palette_path: PathBuf,
    pub animation_path: Option<PathBuf>,
    pub time_of_day_path: Option<PathBuf>,
    pub times: Vec<TimeOfDay>,
    pub weather: Vec<Weather>,
    pub seasons: Vec<Season>,
    pub family_id: Option<String>,
    pub temperature_band: Option<TemperatureBand>,
}

impl SceneManifest {
    /// Parse, contain, and fully decode one package.  Decoding here makes a
    /// registry load a validation boundary rather than a promise that an
    /// invalid asset will fail much later on the Wayland thread.
    pub fn load(path: impl AsRef<Path>) -> Result<(Self, LoadedScene), RegistryError> {
        let manifest_path = path.as_ref();
        let manifest_path = canonicalize_file(manifest_path, "scene manifest")?;
        let package_dir = manifest_path
            .parent()
            .ok_or_else(|| error("scene manifest has no package directory"))?
            .to_path_buf();
        let values = parse_manifest(&read_text_bounded(&manifest_path, MAX_REGISTRY_BYTES)?)?;
        let manifest = Self::from_values(package_dir, values)?;
        let loaded = manifest.load_scene()?;
        Ok((manifest, loaded))
    }

    pub fn load_scene(&self) -> Result<LoadedScene, RegistryError> {
        let image_bytes = read_bounded(&self.image_path, scene::MAX_ENCODED_BYTES, "scene PNG")?;
        let palette_bytes =
            read_bounded(&self.palette_path, scene::MAX_ENCODED_BYTES, "palette PNG")?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.id.hash(&mut hasher);
        image_bytes.hash(&mut hasher);
        palette_bytes.hash(&mut hasher);

        let time_of_day_path = self.time_of_day_path.as_ref();
        if let Some(path) = time_of_day_path {
            read_bounded(path, MAX_REGISTRY_BYTES, "time-of-day manifest")?.hash(&mut hasher);
        }
        let animation = match &self.animation_path {
            Some(path) => {
                let bytes = read_bounded(path, MAX_ANIMATION_BYTES, "animation manifest")?;
                bytes.hash(&mut hasher);
                let input = std::str::from_utf8(&bytes).map_err(|err| {
                    error(format!(
                        "animation manifest {} is not UTF-8: {err}",
                        path.display()
                    ))
                })?;
                Animation::from_toml(input).map_err(|err| error(err.to_string()))?
            }
            None => Animation::static_default(),
        };
        let time_of_day = self
            .time_of_day_path
            .as_ref()
            .map(scene::load_time_of_day)
            .transpose()
            .map_err(|err| error(err.to_string()))?;
        let scene = Scene::with_resources(
            scene::decode_index_png(&image_bytes).map_err(|err| error(err.to_string()))?,
            scene::decode_palette_png(&palette_bytes).map_err(|err| error(err.to_string()))?,
            animation,
            time_of_day,
        )
        .map_err(|err| error(err.to_string()))?;

        Ok(LoadedScene {
            id: self.id.clone(),
            scene,
            content_key: hasher.finish(),
        })
    }

    fn from_values(package_dir: PathBuf, values: ManifestValues) -> Result<Self, RegistryError> {
        let schema_version = values
            .schema_version
            .ok_or_else(|| error("scene schema_version is required"))?;
        if !(MIN_SCENE_SCHEMA_VERSION..=SCENE_SCHEMA_VERSION).contains(&schema_version) {
            return Err(error(format!(
                "unsupported scene schema_version {schema_version}; expected 1 or {SCENE_SCHEMA_VERSION}"
            )));
        }
        let id = values
            .id
            .filter(|value| valid_id(value))
            .ok_or_else(|| error("scene id is required and must be a single safe identifier"))?;
        let image = values
            .image
            .ok_or_else(|| error(format!("scene {id:?}: image is required")))?;
        let palette = values
            .palette
            .ok_or_else(|| error(format!("scene {id:?}: palette is required")))?;
        let image_path = contained_asset(&package_dir, &image, "image")?;
        let palette_path = contained_asset(&package_dir, &palette, "palette")?;
        let animation_path = values
            .animation
            .map(|path| contained_asset(&package_dir, &path, "animation"))
            .transpose()?;
        let time_of_day_path = values
            .time_of_day
            .map(|path| contained_asset(&package_dir, &path, "time_of_day"))
            .transpose()?;
        reject_duplicates(&values.times, "time")?;
        reject_duplicates(&values.weather, "weather")?;
        reject_duplicates(&values.seasons, "season")?;
        let (family_id, temperature_band) = match schema_version {
            1 if values.family_id.is_none() && values.temperature_band.is_none() => (None, None),
            1 => {
                return Err(error(
                    "scene schema_version 1 does not support family metadata",
                ));
            }
            2 => {
                let family_id = values
                    .family_id
                    .filter(|value| valid_id(value))
                    .ok_or_else(|| error("scene schema_version 2 requires a safe family ID"))?;
                let temperature_band = values
                    .temperature_band
                    .ok_or_else(|| error("scene schema_version 2 requires temperature_band"))?;
                (Some(family_id), Some(temperature_band))
            }
            _ => unreachable!("schema version range was checked"),
        };
        Ok(Self {
            schema_version,
            id,
            package_dir,
            image_path,
            palette_path,
            animation_path,
            time_of_day_path,
            times: values.times,
            weather: values.weather,
            seasons: values.seasons,
            family_id,
            temperature_band,
        })
    }

    fn compatible(&self, snapshot: EnvironmentSnapshot) -> bool {
        tag_matches(&self.times, snapshot.time)
            && tag_matches(&self.weather, snapshot.weather)
            && tag_matches(&self.seasons, snapshot.season)
    }

    fn environmental_score(&self, snapshot: EnvironmentSnapshot) -> u32 {
        tag_matches(&self.times, snapshot.time) as u32 * TIME_WEIGHT
            + tag_matches(&self.weather, snapshot.weather) as u32 * WEATHER_WEIGHT
            + tag_matches(&self.seasons, snapshot.season) as u32 * SEASON_WEIGHT
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedScene {
    pub id: String,
    pub scene: Scene,
    /// Stable for the decoded package contents.  A renderer can use this to
    /// avoid a redundant upload when a selection keeps the same content.
    pub content_key: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneFamily {
    temperature_band: TemperatureBand,
    variants: BTreeMap<Weather, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneRegistry {
    scenes: BTreeMap<String, SceneManifest>,
    /// Pool-sourced packages that are manually selectable but never automatic
    /// candidates (kept out of `scenes` so adaptive validation is unchanged).
    extras: BTreeMap<String, SceneManifest>,
    fallback: Option<String>,
    families: BTreeMap<String, SceneFamily>,
}

impl SceneRegistry {
    /// Load `registry.toml` and the listed `<id>/scene.toml` packages below
    /// `root`.  The explicit list prevents accidental package discovery and
    /// makes a missing/ambiguous registry state observable.
    pub fn load(root: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let root = canonicalize_dir(root.as_ref(), "scene registry root")?;
        let config = parse_registry(&read_text_bounded(
            &root.join("registry.toml"),
            MAX_REGISTRY_BYTES,
        )?)?;
        if config.scenes.is_empty() {
            return Err(error("scene registry must list at least one scene"));
        }
        let mut manifests = Vec::with_capacity(config.scenes.len());
        let mut listed = BTreeSet::new();
        for id in config.scenes {
            if !valid_id(&id) {
                return Err(error(format!(
                    "registry scene id {id:?} is not a safe identifier"
                )));
            }
            if !listed.insert(id.clone()) {
                return Err(error(format!("registry scene id {id:?} is duplicated")));
            }
            let package_dir = canonicalize_dir(&root.join(&id), "scene package")?;
            if !package_dir.starts_with(&root) {
                return Err(error(format!(
                    "scene package {id:?} escapes the registry root"
                )));
            }
            let manifest_path = package_dir.join("scene.toml");
            let (manifest, _) = SceneManifest::load(&manifest_path)?;
            if manifest.id != id {
                return Err(error(format!(
                    "registry entry {id:?} contains manifest id {:?}",
                    manifest.id
                )));
            }
            if !manifest.package_dir.starts_with(&root) {
                return Err(error(format!(
                    "scene manifest {id:?} resolves outside the registry root"
                )));
            }
            manifests.push(manifest);
        }
        Self::from_manifests(config.fallback, manifests)
    }

    pub fn from_manifests(
        fallback: Option<String>,
        manifests: impl IntoIterator<Item = SceneManifest>,
    ) -> Result<Self, RegistryError> {
        let mut scenes = BTreeMap::new();
        let mut families: BTreeMap<String, SceneFamily> = BTreeMap::new();
        for manifest in manifests {
            if let (Some(family_id), Some(temperature_band)) =
                (&manifest.family_id, manifest.temperature_band)
            {
                let family = families
                    .entry(family_id.clone())
                    .or_insert_with(|| SceneFamily {
                        temperature_band,
                        variants: BTreeMap::new(),
                    });
                if family.temperature_band != temperature_band {
                    return Err(error(format!(
                        "family {family_id:?} has inconsistent temperature bands"
                    )));
                }
                if manifest.weather.len() == 1 {
                    let weather = manifest.weather[0];
                    if matches!(
                        weather,
                        Weather::Clear | Weather::Cloudy | Weather::Rain | Weather::Snow
                    ) && family
                        .variants
                        .insert(weather, manifest.id.clone())
                        .is_some()
                    {
                        return Err(error(format!(
                            "family {family_id:?} has duplicate {weather:?} variants"
                        )));
                    }
                }
            }
            if scenes.insert(manifest.id.clone(), manifest).is_some() {
                return Err(error("scene manifest IDs must be unique"));
            }
        }
        if let Some(fallback_id) = &fallback {
            if !scenes.contains_key(fallback_id) {
                return Err(error(format!(
                    "designated fallback {fallback_id:?} is not in the registry"
                )));
            }
        } else if !scenes.is_empty() {
            return Err(error("a non-empty scene registry requires a fallback"));
        }
        Ok(Self {
            scenes,
            extras: BTreeMap::new(),
            fallback,
            families,
        })
    }

    /// Empty is useful as a pure selector input and is not accepted by the
    /// on-disk loader, which requires a validated offline fallback.
    pub fn empty() -> Self {
        Self {
            scenes: BTreeMap::new(),
            extras: BTreeMap::new(),
            fallback: None,
            families: BTreeMap::new(),
        }
    }

    /// Attach validated pool-sourced packages. In non-adaptive pool mode the
    /// packages join the automatic candidate set (the union of enabled pools
    /// is then narrowed by the shared eligibility gate). In adaptive mode they
    /// stay manual-only: non-adaptive library entries must never become
    /// adaptive families and CR-0006 registry validation is unchanged.
    pub fn add_pool_packages(
        &mut self,
        manifests: impl IntoIterator<Item = SceneManifest>,
        automatic: bool,
    ) -> Result<(), RegistryError> {
        for manifest in manifests {
            if self.scenes.contains_key(&manifest.id) || self.extras.contains_key(&manifest.id) {
                return Err(error(format!(
                    "pool package {:?} duplicates a known scene",
                    manifest.id
                )));
            }
            if automatic {
                self.scenes.insert(manifest.id.clone(), manifest);
            } else {
                self.extras.insert(manifest.id.clone(), manifest);
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&SceneManifest> {
        self.scenes.get(id).or_else(|| self.extras.get(id))
    }

    pub fn len(&self) -> usize {
        self.scenes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }

    pub fn fallback_id(&self) -> Option<&str> {
        self.fallback.as_deref()
    }

    /// Validate the stronger metadata contract required by the opt-in
    /// weather-adaptive policy. Ordinary schema-v1/v2 registries are not
    /// required to opt in or carry family metadata.
    pub fn validate_adaptive(&self) -> Result<(), RegistryError> {
        if self.scenes.is_empty() {
            return Err(error("adaptive selection requires a non-empty registry"));
        }
        for manifest in self.scenes.values() {
            let (Some(family_id), Some(band)) = (&manifest.family_id, manifest.temperature_band)
            else {
                return Err(error(format!(
                    "adaptive selection requires family metadata for scene {:?}",
                    manifest.id
                )));
            };
            if !valid_id(family_id)
                || manifest.weather.len() != 1
                || !matches!(
                    manifest.weather[0],
                    Weather::Clear | Weather::Cloudy | Weather::Rain | Weather::Snow
                )
            {
                return Err(error(format!(
                    "adaptive scene {:?} requires one clear/cloudy/rain/snow variant tag",
                    manifest.id
                )));
            }
            let family = self
                .families
                .get(family_id)
                .ok_or_else(|| error(format!("adaptive family {family_id:?} is empty")))?;
            if family.temperature_band != band {
                return Err(error(format!(
                    "adaptive family {family_id:?} has inconsistent metadata"
                )));
            }
        }
        for family_id in self.families.keys() {
            if !self.families[family_id]
                .variants
                .contains_key(&Weather::Clear)
            {
                return Err(error(format!(
                    "adaptive family {family_id:?} requires a clear fallback variant"
                )));
            }
        }
        for required in [
            TemperatureBand::Cold,
            TemperatureBand::Cool,
            TemperatureBand::Mild,
            TemperatureBand::Warm,
            TemperatureBand::Hot,
        ] {
            if !self
                .families
                .values()
                .any(|family| family.temperature_band == required)
            {
                return Err(error(format!(
                    "adaptive registry has no family for the {:?} temperature band",
                    required
                )));
            }
        }
        let fallback = self
            .fallback
            .as_deref()
            .and_then(|id| self.scenes.get(id))
            .ok_or_else(|| error("adaptive registry requires a valid V29 offline fallback"))?;
        if fallback.id != "v29-seascape"
            || fallback.family_id.as_deref() != Some("v29-seascape")
            || fallback.temperature_band != Some(TemperatureBand::Warm)
        {
            return Err(error(
                "adaptive registry fallback must be the clear V29 Seascape package",
            ));
        }
        Ok(())
    }

    pub fn adaptive_state_for_scene(&self, scene_id: &str) -> Option<AdaptiveSelectionState> {
        let manifest = self.scenes.get(scene_id)?;
        Some(AdaptiveSelectionState {
            family_id: manifest.family_id.clone()?,
            temperature_band: manifest.temperature_band?,
        })
    }

    pub fn adaptive_full_selection(
        &self,
        temperature_c: Option<f32>,
        appearance: PrecipitationAppearance,
        previous: Option<&AdaptiveSelectionState>,
        active_scene_id: Option<&str>,
        gate: Option<&PoolGate<'_>>,
    ) -> Result<AdaptiveSelectionDecision, RegistryError> {
        self.validate_adaptive()?;
        let previous = previous.filter(|state| {
            self.families
                .get(&state.family_id)
                .is_some_and(|family| family.temperature_band == state.temperature_band)
        });
        let fallback_id = self
            .fallback
            .as_deref()
            .ok_or_else(|| error("adaptive registry has no offline fallback"))?;
        let fallback_state = self
            .adaptive_state_for_scene(fallback_id)
            .ok_or_else(|| error("adaptive fallback has no family metadata"))?;
        let valid_temperature =
            temperature_c.filter(|value| value.is_finite() && (-100.0..=100.0).contains(value));
        let target_band = match (valid_temperature, previous) {
            (Some(value), Some(state)) => {
                TemperatureBand::with_hysteresis(value, Some(state.temperature_band))
                    .unwrap_or(state.temperature_band)
            }
            (Some(value), None) => TemperatureBand::classify(value)
                .ok_or_else(|| error("adaptive temperature cannot be classified"))?,
            (None, Some(state)) => state.temperature_band,
            (None, None) => fallback_state.temperature_band,
        };
        let family_id =
            if let Some(state) = previous.filter(|state| state.temperature_band == target_band) {
                state.family_id.as_str()
            } else if valid_temperature.is_none() && previous.is_none() {
                fallback_state.family_id.as_str()
            } else {
                self.best_family_for_band(target_band, appearance)
                    .ok_or_else(|| error(format!("no family matches {:?} band", target_band)))?
            };
        let active_in_family = active_scene_id.filter(|id| {
            self.scenes
                .get(*id)
                .and_then(|manifest| manifest.family_id.as_deref())
                == Some(family_id)
        });
        let scene_id = if let Some(gate) = gate {
            // CR-0006 composition is unchanged: the family was already chosen;
            // only its ordered candidates are filtered in the existing
            // appearance-preference order. A rejected candidate never crosses
            // to another family/band and never mutates stored state.
            let mut candidates: Vec<&str> = Vec::new();
            if appearance == PrecipitationAppearance::Unknown {
                candidates.extend(active_in_family);
            }
            if let Some(family) = self.families.get(family_id) {
                for weather in appearance.preferences() {
                    if let Some(id) = family.variants.get(weather) {
                        candidates.push(id);
                    }
                }
            }
            candidates.extend(active_in_family);
            candidates
                .into_iter()
                .find(|id| gate.is_eligible(id))
                .ok_or_else(|| error("no eligible candidate"))?
                .to_owned()
        } else {
            if appearance == PrecipitationAppearance::Unknown {
                active_in_family.map(str::to_owned)
            } else {
                None
            }
            .or_else(|| self.variant_id(family_id, appearance).map(str::to_owned))
            .or_else(|| active_in_family.map(str::to_owned))
            .ok_or_else(|| error(format!("family {family_id:?} has no usable variant")))?
        };
        let state = self
            .adaptive_state_for_scene(&scene_id)
            .ok_or_else(|| error(format!("selected scene {scene_id:?} lacks family metadata")))?;
        Ok(AdaptiveSelectionDecision {
            scene_id,
            state,
            rationale: format!(
                "temperature band {:?}; family held or selected by policy",
                target_band
            ),
        })
    }

    pub fn adaptive_variant_in_family(
        &self,
        family_id: &str,
        appearance: PrecipitationAppearance,
        gate: Option<&PoolGate<'_>>,
    ) -> Option<&SceneManifest> {
        if appearance == PrecipitationAppearance::Unknown {
            return None;
        }
        let family = self.families.get(family_id)?;
        // Walk the approved same-family fallback order and permit only
        // eligible variants; nothing crosses to another family.
        appearance
            .preferences()
            .iter()
            .filter_map(|weather| family.variants.get(weather))
            .find(|id| gate.is_none_or(|gate| gate.is_eligible(id)))
            .and_then(|id| self.scenes.get(id))
    }

    fn best_family_for_band(
        &self,
        band: TemperatureBand,
        appearance: PrecipitationAppearance,
    ) -> Option<&str> {
        self.families
            .iter()
            .filter(|(_, family)| family.temperature_band == band)
            .filter_map(|(id, family)| {
                let rank = appearance
                    .preferences()
                    .iter()
                    .position(|weather| family.variants.contains_key(weather))?;
                Some((rank, id.as_str()))
            })
            .min_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)))
            .map(|(_, id)| id)
    }

    fn variant_id(&self, family_id: &str, appearance: PrecipitationAppearance) -> Option<&str> {
        let family = self.families.get(family_id)?;
        appearance
            .preferences()
            .iter()
            .find_map(|weather| family.variants.get(weather))
            .map(String::as_str)
    }

    pub fn manifests(&self) -> impl Iterator<Item = &SceneManifest> {
        self.scenes.values()
    }

    pub fn select(
        &self,
        snapshot: EnvironmentSnapshot,
        seed: u64,
        trigger: SelectionTrigger,
        gate: Option<&PoolGate<'_>>,
    ) -> SelectionResult {
        match trigger {
            SelectionTrigger::ManualId(id) => {
                // Manual IDs are the explicit override: pool and availability
                // rules never block them, but the package must still be known.
                if self.scenes.contains_key(&id) || self.extras.contains_key(&id) {
                    SelectionResult::Selected(SelectionDecision {
                        id: id.clone(),
                        trigger: SelectionTrigger::ManualId(id),
                        match_score: None,
                        random_value: None,
                        rationale: "manual scene ID".into(),
                    })
                } else {
                    SelectionResult::Rejected(format!("scene ID {id:?} is not registered"))
                }
            }
            SelectionTrigger::Next => self.select_next(snapshot, None, gate),
            SelectionTrigger::Startup | SelectionTrigger::Resume | SelectionTrigger::Reload => {
                self.select_scored(snapshot, seed, trigger, gate)
            }
            SelectionTrigger::WeatherUpdated
            | SelectionTrigger::ClockTick
            | SelectionTrigger::OutputChanged
            | SelectionTrigger::ConfigChanged => SelectionResult::Ignored,
        }
    }

    fn select_scored(
        &self,
        snapshot: EnvironmentSnapshot,
        seed: u64,
        trigger: SelectionTrigger,
        gate: Option<&PoolGate<'_>>,
    ) -> SelectionResult {
        let mut candidates = self
            .scenes
            .values()
            .filter(|manifest| gate.is_none_or(|gate| gate.is_eligible(&manifest.id)))
            .map(|manifest| {
                let match_score = manifest.environmental_score(snapshot);
                let random_value = deterministic_unit(seed, &manifest.id);
                (manifest, match_score, random_value)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            if gate.is_some() {
                // An empty eligible set never reaches the designated fallback:
                // report that no scene is eligible and let the caller retain a
                // resident scene or fail closed at startup.
                return SelectionResult::NoScene("no eligible scene".into());
            }
            return self
                .fallback
                .as_ref()
                .map(|id| {
                    SelectionResult::Selected(SelectionDecision {
                        id: id.clone(),
                        trigger,
                        match_score: None,
                        random_value: None,
                        rationale: "designated fallback; no compatible scene".into(),
                    })
                })
                .unwrap_or(SelectionResult::NoScene(
                    "registry is empty or has no fallback".into(),
                ));
        }
        if candidates.iter().all(|(_, score, _)| *score == 0) {
            // The fallback is selectable only inside the eligible set; it is
            // never a bypass around the pool/rules filter.
            let fallback = self
                .fallback
                .as_ref()
                .filter(|id| gate.is_none_or(|gate| gate.is_eligible(id.as_str())));
            if let Some(fallback) = fallback {
                return SelectionResult::Selected(SelectionDecision {
                    id: fallback.clone(),
                    trigger,
                    match_score: None,
                    random_value: None,
                    rationale: "designated fallback; no environmental tag matched".into(),
                });
            }
            if gate.is_none() {
                // Legacy behavior: a validated registry always permits this
                // branch to reach its fallback.
                unreachable!("validated registry fallback is missing")
            }
        }
        candidates.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| right.2.total_cmp(&left.2))
                .then_with(|| left.0.id.cmp(&right.0.id))
        });
        let (manifest, match_score, random_value) = candidates[0];
        SelectionResult::Selected(SelectionDecision {
            id: manifest.id.clone(),
            trigger,
            match_score: Some(match_score),
            random_value: Some(random_value),
            rationale: format!("highest tag score {match_score} with deterministic seed"),
        })
    }

    fn select_next(
        &self,
        snapshot: EnvironmentSnapshot,
        current_id: Option<&str>,
        gate: Option<&PoolGate<'_>>,
    ) -> SelectionResult {
        let compatible = self
            .scenes
            .values()
            .filter(|manifest| manifest.compatible(snapshot))
            .filter(|manifest| gate.is_none_or(|gate| gate.is_eligible(&manifest.id)))
            .map(|manifest| manifest.id.as_str())
            .collect::<Vec<_>>();
        if compatible.is_empty() {
            return if gate.is_some() {
                // With pool rules configured, `next` is a no-op with a bounded
                // reason; it never cycles to an ineligible scene.
                SelectionResult::Noop("no eligible scene for next".into())
            } else {
                SelectionResult::NoScene("no compatible scene for next".into())
            };
        }
        if compatible.len() == 1 && current_id == Some(compatible[0]) {
            return SelectionResult::Noop("only the active scene is compatible".into());
        }
        let start = current_id
            .and_then(|id| compatible.iter().position(|candidate| *candidate == id))
            .map_or(0, |position| (position + 1) % compatible.len());
        let id = compatible[start].to_owned();
        SelectionResult::Selected(SelectionDecision {
            id,
            trigger: SelectionTrigger::Next,
            match_score: None,
            random_value: None,
            rationale: "next compatible scene in stable ID order".into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionTrigger {
    Startup,
    Resume,
    Reload,
    ManualId(String),
    Next,
    WeatherUpdated,
    ClockTick,
    OutputChanged,
    ConfigChanged,
}

impl SelectionTrigger {
    pub fn is_explicit(&self) -> bool {
        matches!(
            self,
            Self::Startup | Self::Resume | Self::Reload | Self::ManualId(_) | Self::Next
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectionDecision {
    pub id: String,
    pub trigger: SelectionTrigger,
    pub match_score: Option<u32>,
    pub random_value: Option<f64>,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectionResult {
    Selected(SelectionDecision),
    Noop(String),
    NoScene(String),
    Rejected(String),
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionRequest {
    pub generation: u64,
    pub id: String,
    pub trigger: SelectionTrigger,
    pub rationale: String,
}

#[derive(Debug, Clone)]
pub struct SelectionController {
    registry: SceneRegistry,
    pools: Option<ScenePools>,
    active_id: Option<String>,
    next_generation: u64,
    selection_generation: u64,
}

impl SelectionController {
    pub fn new(registry: SceneRegistry) -> Self {
        Self {
            registry,
            pools: None,
            active_id: None,
            next_generation: 0,
            selection_generation: 0,
        }
    }

    /// Install (or clear) the validated pool/rule state. This never selects;
    /// later automatic requests use one current environment snapshot.
    pub fn set_pools(&mut self, pools: Option<ScenePools>) {
        self.pools = pools;
    }

    pub fn pools(&self) -> Option<&ScenePools> {
        self.pools.as_ref()
    }

    pub fn request(
        &mut self,
        snapshot: EnvironmentSnapshot,
        temperature_c: Option<f32>,
        seed: u64,
        trigger: SelectionTrigger,
    ) -> Result<SelectionRequest, SelectionResult> {
        if !trigger.is_explicit() {
            return Err(SelectionResult::Ignored);
        }
        let environment = PoolEnvironment::new(&snapshot, temperature_c);
        let gate = self.pools.as_ref().map(|pools| PoolGate {
            pools,
            environment: &environment,
        });
        let result = match trigger {
            SelectionTrigger::Next => {
                self.registry
                    .select_next(snapshot, self.active_id.as_deref(), gate.as_ref())
            }
            trigger => self.registry.select(snapshot, seed, trigger, gate.as_ref()),
        };
        match result {
            SelectionResult::Selected(decision) => {
                self.next_generation = self.next_generation.saturating_add(1);
                self.selection_generation = self.selection_generation.saturating_add(1);
                Ok(SelectionRequest {
                    generation: self.next_generation,
                    id: decision.id,
                    trigger: decision.trigger,
                    rationale: decision.rationale,
                })
            }
            other => Err(other),
        }
    }

    pub(crate) fn request_selected(
        &mut self,
        id: &str,
        trigger: SelectionTrigger,
        rationale: impl Into<String>,
    ) -> Result<SelectionRequest, SelectionResult> {
        if !trigger.is_explicit() && trigger != SelectionTrigger::WeatherUpdated {
            return Err(SelectionResult::Ignored);
        }
        if self.registry.get(id).is_none() {
            return Err(SelectionResult::Rejected(format!(
                "scene ID {id:?} is not registered"
            )));
        }
        self.next_generation = self.next_generation.saturating_add(1);
        self.selection_generation = self.selection_generation.saturating_add(1);
        Ok(SelectionRequest {
            generation: self.next_generation,
            id: id.to_owned(),
            trigger,
            rationale: rationale.into(),
        })
    }

    pub fn mark_active(&mut self, id: &str) -> Result<(), RegistryError> {
        if self.registry.get(id).is_none() {
            return Err(error(format!("cannot activate unregistered scene {id:?}")));
        }
        self.active_id = Some(id.to_owned());
        Ok(())
    }

    pub fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    pub fn generation(&self) -> u64 {
        self.selection_generation
    }

    pub(crate) fn request_generation(&self) -> u64 {
        self.next_generation
    }

    pub fn invalidate_requests(&mut self) {
        self.next_generation = self.next_generation.saturating_add(1);
    }

    pub fn registry(&self) -> &SceneRegistry {
        &self.registry
    }

    pub fn replace_registry(&mut self, registry: SceneRegistry) {
        let active_id = self.active_id.clone();
        self.registry = registry;
        self.active_id = active_id.filter(|id| self.registry.get(id).is_some());
    }

    /// Install one complete validated candidate (registry plus pool state) in
    /// a single step so a config reload can never mix generations.
    pub fn replace_sources(&mut self, registry: SceneRegistry, pools: Option<ScenePools>) {
        self.replace_registry(registry);
        self.set_pools(pools);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneLoadToken {
    pub generation: u64,
    pub id: String,
}

pub struct SceneLoadJob {
    pub request: SelectionRequest,
    pub token: SceneLoadToken,
    pub manifest: SceneManifest,
    pub resulting_state: Option<AdaptiveSelectionState>,
}

pub struct SceneLoadCompletion {
    pub request: SelectionRequest,
    pub token: SceneLoadToken,
    pub candidate: Result<LoadedScene, RegistryError>,
    pub resulting_state: Option<AdaptiveSelectionState>,
}

/// One bounded background worker for scene decoding. Wayland and renderer
/// state never leave the event-loop owner; only validated package manifests
/// and decoded CPU scene bundles cross this channel.
pub struct SceneLoadWorker {
    jobs: SyncSender<SceneLoadJob>,
    completions: Receiver<SceneLoadCompletion>,
    wakeups: Option<Channel<()>>,
}

impl SceneLoadWorker {
    pub fn new() -> Result<Self, RegistryError> {
        Self::with_loader(|manifest| manifest.load_scene())
    }

    fn with_loader(
        loader: impl Fn(SceneManifest) -> Result<LoadedScene, RegistryError> + Send + 'static,
    ) -> Result<Self, RegistryError> {
        let (jobs, job_receiver) = mpsc::sync_channel::<SceneLoadJob>(1);
        let (completion_sender, completions) = mpsc::sync_channel::<SceneLoadCompletion>(1);
        let (wakeup_sender, wakeups) = channel::sync_channel(1);
        thread::Builder::new()
            .name("worldwall-scene-load".to_owned())
            .spawn(move || {
                while let Ok(job) = job_receiver.recv() {
                    let candidate = loader(job.manifest);
                    if completion_sender
                        .send(SceneLoadCompletion {
                            request: job.request,
                            token: job.token,
                            candidate,
                            resulting_state: job.resulting_state,
                        })
                        .is_err()
                    {
                        break;
                    }
                    // The owner is woken even when no frame or weather timer is
                    // armed, so final IPC results are not tied to presentation.
                    let _ = wakeup_sender.try_send(());
                }
            })
            .map_err(|error_value| {
                error(format!("starting scene-load worker failed: {error_value}"))
            })?;
        Ok(Self {
            jobs,
            completions,
            wakeups: Some(wakeups),
        })
    }

    pub fn submit(&self, job: SceneLoadJob) -> Result<(), RegistryError> {
        self.jobs
            .try_send(job)
            .map_err(|error_value| match error_value {
                TrySendError::Full(_) => error("scene-load worker queue is full"),
                TrySendError::Disconnected(_) => error("scene-load worker is unavailable"),
            })
    }

    pub fn try_recv(&self) -> Option<SceneLoadCompletion> {
        self.completions.try_recv().ok()
    }

    pub fn take_wakeup_channel(&mut self) -> Option<Channel<()>> {
        self.wakeups.take()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    Applied {
        id: String,
        generation: u64,
    },
    Unchanged {
        id: String,
        generation: u64,
    },
    Stale {
        generation: u64,
    },
    Failed {
        id: String,
        retained_id: Option<String>,
        reason: String,
    },
}

#[derive(Debug, Default)]
pub struct SceneTransaction {
    active: Option<LoadedScene>,
    latest_generation: u64,
}

impl SceneTransaction {
    pub fn active(&self) -> Option<&LoadedScene> {
        self.active.as_ref()
    }

    pub fn invalidate_through(&mut self, generation: u64) {
        self.latest_generation = self.latest_generation.max(generation);
    }

    pub fn begin(&mut self, request: &SelectionRequest) -> SceneLoadToken {
        self.latest_generation = self.latest_generation.max(request.generation);
        SceneLoadToken {
            generation: request.generation,
            id: request.id.clone(),
        }
    }

    pub fn complete(
        &mut self,
        token: SceneLoadToken,
        candidate: Result<LoadedScene, RegistryError>,
    ) -> CommitOutcome {
        self.complete_with_swap(token, candidate, |_| Ok::<(), RegistryError>(()))
    }

    /// Complete a candidate only after the renderer accepts its scene bundle.
    ///
    /// The callback is the renderer-facing commit boundary. It is deliberately
    /// called only for a current, changed candidate; stale work and unchanged
    /// content never reach the renderer. If the callback fails, `active` is
    /// untouched so the previous valid scene remains authoritative.
    pub fn complete_with_swap<E>(
        &mut self,
        token: SceneLoadToken,
        candidate: Result<LoadedScene, RegistryError>,
        swap: impl FnOnce(&Scene) -> Result<(), E>,
    ) -> CommitOutcome
    where
        E: fmt::Display,
    {
        if token.generation != self.latest_generation {
            return CommitOutcome::Stale {
                generation: token.generation,
            };
        }
        match candidate {
            Ok(candidate) if candidate.id == token.id => {
                if self.active.as_ref().is_some_and(|active| {
                    active.id == candidate.id && active.content_key == candidate.content_key
                }) {
                    return CommitOutcome::Unchanged {
                        id: candidate.id,
                        generation: token.generation,
                    };
                }
                if let Err(error) = swap(&candidate.scene) {
                    return CommitOutcome::Failed {
                        id: token.id,
                        retained_id: self.active.as_ref().map(|scene| scene.id.clone()),
                        reason: format!("renderer scene swap failed: {error}"),
                    };
                }
                let id = candidate.id.clone();
                self.active = Some(candidate);
                CommitOutcome::Applied {
                    id,
                    generation: token.generation,
                }
            }
            Ok(candidate) => CommitOutcome::Failed {
                id: token.id,
                retained_id: self.active.as_ref().map(|scene| scene.id.clone()),
                reason: format!("candidate ID {:?} does not match request", candidate.id),
            },
            Err(error) => CommitOutcome::Failed {
                id: token.id,
                retained_id: self.active.as_ref().map(|scene| scene.id.clone()),
                reason: error.to_string(),
            },
        }
    }
}

#[derive(Default)]
struct ManifestValues {
    schema_version: Option<u32>,
    id: Option<String>,
    image: Option<String>,
    palette: Option<String>,
    animation: Option<String>,
    time_of_day: Option<String>,
    times: Vec<TimeOfDay>,
    weather: Vec<Weather>,
    seasons: Vec<Season>,
    family_id: Option<String>,
    temperature_band: Option<TemperatureBand>,
    times_seen: bool,
    weather_seen: bool,
    seasons_seen: bool,
}

#[derive(Default)]
struct RegistryValues {
    schema_version: Option<u32>,
    fallback: Option<String>,
    scenes: Vec<String>,
    scenes_seen: bool,
}

fn parse_manifest(input: &str) -> Result<ManifestValues, RegistryError> {
    let mut values = ManifestValues::default();
    for (line_number, raw_line) in input.lines().enumerate() {
        let line_number = line_number + 1;
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = assignment(line, line_number)?;
        match key {
            "schema_version" => assign_once(
                &mut values.schema_version,
                parse_u32(value, line_number, key)?,
                key,
                line_number,
            )?,
            "id" => assign_once(
                &mut values.id,
                parse_string(value, line_number, key)?,
                key,
                line_number,
            )?,
            "image" => assign_once(
                &mut values.image,
                parse_string(value, line_number, key)?,
                key,
                line_number,
            )?,
            "palette" => assign_once(
                &mut values.palette,
                parse_string(value, line_number, key)?,
                key,
                line_number,
            )?,
            "animation" => assign_once(
                &mut values.animation,
                parse_string(value, line_number, key)?,
                key,
                line_number,
            )?,
            "time_of_day" => assign_once(
                &mut values.time_of_day,
                parse_string(value, line_number, key)?,
                key,
                line_number,
            )?,
            "family" => assign_once(
                &mut values.family_id,
                parse_string(value, line_number, key)?,
                key,
                line_number,
            )?,
            "temperature_band" => {
                let value = parse_string(value, line_number, key)?;
                let band = TemperatureBand::parse(&value).ok_or_else(|| {
                    error(format!(
                        "line {line_number}: unsupported temperature_band {value:?}"
                    ))
                })?;
                assign_once(&mut values.temperature_band, band, key, line_number)?;
            }
            "times" => {
                if values.times_seen {
                    return Err(error(format!("line {line_number}: duplicate key {key:?}")));
                }
                values.times_seen = true;
                values.times = parse_tags(value, line_number, key, parse_time)?;
            }
            "weather" => {
                if values.weather_seen {
                    return Err(error(format!("line {line_number}: duplicate key {key:?}")));
                }
                values.weather_seen = true;
                values.weather = parse_tags(value, line_number, key, parse_weather)?;
            }
            "seasons" => {
                if values.seasons_seen {
                    return Err(error(format!("line {line_number}: duplicate key {key:?}")));
                }
                values.seasons_seen = true;
                values.seasons = parse_tags(value, line_number, key, parse_season)?;
            }
            other => {
                return Err(error(format!(
                    "line {line_number}: unknown scene key {other:?}"
                )));
            }
        }
    }
    Ok(values)
}

fn parse_registry(input: &str) -> Result<RegistryValues, RegistryError> {
    let mut values = RegistryValues::default();
    for (line_number, raw_line) in input.lines().enumerate() {
        let line_number = line_number + 1;
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = assignment(line, line_number)?;
        match key {
            "schema_version" => assign_once(
                &mut values.schema_version,
                parse_u32(value, line_number, key)?,
                key,
                line_number,
            )?,
            "fallback" => assign_once(
                &mut values.fallback,
                parse_string(value, line_number, key)?,
                key,
                line_number,
            )?,
            "scenes" => {
                if values.scenes_seen {
                    return Err(error(format!(
                        "line {line_number}: duplicate registry key {key:?}"
                    )));
                }
                values.scenes_seen = true;
                values.scenes = parse_string_array(value, line_number, key)?;
            }
            other => {
                return Err(error(format!(
                    "line {line_number}: unknown registry key {other:?}"
                )));
            }
        }
    }
    match values.schema_version {
        Some(REGISTRY_SCHEMA_VERSION) => {}
        Some(version) => {
            return Err(error(format!(
                "unsupported registry schema_version {version}; expected {REGISTRY_SCHEMA_VERSION}"
            )));
        }
        None => return Err(error("registry schema_version is required")),
    }
    if values.fallback.as_deref().is_none_or(|id| !valid_id(id)) {
        return Err(error(
            "registry fallback is required and must be a safe identifier",
        ));
    }
    Ok(values)
}

fn assignment(line: &str, line_number: usize) -> Result<(&str, &str), RegistryError> {
    let (key, value) = line
        .split_once('=')
        .ok_or_else(|| error(format!("line {line_number}: expected key = value")))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(error(format!("line {line_number}: empty key")));
    }
    Ok((key, value.trim()))
}

fn assign_once<T>(
    slot: &mut Option<T>,
    value: T,
    key: &str,
    line: usize,
) -> Result<(), RegistryError> {
    if slot.is_some() {
        return Err(error(format!("line {line}: duplicate key {key:?}")));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_tags<T: Copy>(
    value: &str,
    line: usize,
    key: &str,
    parse: fn(&str) -> Option<T>,
) -> Result<Vec<T>, RegistryError> {
    parse_string_array(value, line, key)?
        .into_iter()
        .map(|tag| {
            parse(&tag).ok_or_else(|| error(format!("line {line}: unsupported {key} tag {tag:?}")))
        })
        .collect()
}

fn parse_time(value: &str) -> Option<TimeOfDay> {
    match value {
        "dawn" => Some(TimeOfDay::Dawn),
        "day" => Some(TimeOfDay::Day),
        "dusk" => Some(TimeOfDay::Dusk),
        "night" => Some(TimeOfDay::Night),
        "unknown" => Some(TimeOfDay::Unknown),
        _ => None,
    }
}

fn parse_weather(value: &str) -> Option<Weather> {
    match value {
        "clear" => Some(Weather::Clear),
        "cloudy" => Some(Weather::Cloudy),
        "rain" => Some(Weather::Rain),
        "snow" => Some(Weather::Snow),
        "storm" => Some(Weather::Storm),
        "fog" => Some(Weather::Fog),
        "unknown" => Some(Weather::Unknown),
        _ => None,
    }
}

fn parse_season(value: &str) -> Option<Season> {
    match value {
        "spring" => Some(Season::Spring),
        "summer" => Some(Season::Summer),
        "autumn" => Some(Season::Autumn),
        "winter" => Some(Season::Winter),
        "unknown" => Some(Season::Unknown),
        _ => None,
    }
}

fn parse_string(value: &str, line: usize, key: &str) -> Result<String, RegistryError> {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .filter(|value| !value.contains('"') && !value.contains('\n'))
        .map(str::to_owned)
        .ok_or_else(|| error(format!("line {line}: {key} must be a quoted string")))
}

fn parse_string_array(value: &str, line: usize, key: &str) -> Result<Vec<String>, RegistryError> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| error(format!("line {line}: {key} must be an array")))?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|item| parse_string(item.trim(), line, key))
        .collect()
}

fn parse_u32(value: &str, line: usize, key: &str) -> Result<u32, RegistryError> {
    value
        .parse::<u32>()
        .map_err(|_| error(format!("line {line}: {key} must be an unsigned integer")))
}

fn contained_asset(
    package_dir: &Path,
    relative: &str,
    label: &str,
) -> Result<PathBuf, RegistryError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(error(format!(
            "{label} path {relative:?} must stay inside the scene package"
        )));
    }
    let resolved = canonicalize_file(&package_dir.join(path), label)?;
    if !resolved.starts_with(package_dir) {
        return Err(error(format!(
            "{label} path {relative:?} escapes the scene package"
        )));
    }
    Ok(resolved)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn reject_duplicates<T: Ord + Copy>(values: &[T], label: &str) -> Result<(), RegistryError> {
    let mut seen = BTreeSet::new();
    if values.iter().any(|value| !seen.insert(*value)) {
        return Err(error(format!("duplicate {label} tag")));
    }
    Ok(())
}

fn tag_matches<T: Eq + Copy>(tags: &[T], value: T) -> bool {
    tags.is_empty() || tags.contains(&value)
}

fn deterministic_unit(seed: u64, id: &str) -> f64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    id.hash(&mut hasher);
    let value = hasher.finish() >> 11;
    (value as f64) / ((1u64 << 53) as f64)
}

fn canonicalize_dir(path: &Path, label: &str) -> Result<PathBuf, RegistryError> {
    let path = path.canonicalize().map_err(|err| {
        error(format!(
            "canonicalizing {label} {} failed: {err}",
            path.display()
        ))
    })?;
    if !path.is_dir() {
        return Err(error(format!(
            "{label} {} is not a directory",
            path.display()
        )));
    }
    Ok(path)
}

fn canonicalize_file(path: &Path, label: &str) -> Result<PathBuf, RegistryError> {
    let path = path.canonicalize().map_err(|err| {
        error(format!(
            "canonicalizing {label} {} failed: {err}",
            path.display()
        ))
    })?;
    if !path.is_file() {
        return Err(error(format!("{label} {} is not a file", path.display())));
    }
    Ok(path)
}

fn read_text_bounded(path: &Path, max: usize) -> Result<String, RegistryError> {
    let bytes = read_bounded(path, max, "text file")?;
    String::from_utf8(bytes).map_err(|err| error(format!("{} is not UTF-8: {err}", path.display())))
}

fn read_bounded(path: &Path, max: usize, label: &str) -> Result<Vec<u8>, RegistryError> {
    let file = File::open(path)
        .map_err(|err| error(format!("reading {label} {} failed: {err}", path.display())))?;
    let length = file
        .metadata()
        .map(|metadata| metadata.len() as usize)
        .unwrap_or(0);
    let mut bytes = Vec::with_capacity(max.min(length));
    file.take((max as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| error(format!("reading {label} {} failed: {err}", path.display())))?;
    if bytes.len() > max {
        return Err(error(format!(
            "{label} {} exceeds {max} bytes",
            path.display()
        )));
    }
    Ok(bytes)
}

fn error(message: impl Into<String>) -> RegistryError {
    RegistryError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::{BitDepth, ColorType, Encoder};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn package_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "worldwall-s05-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn png_scene(indices: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, indices.len() as u32, 1);
            encoder.set_color(ColorType::Grayscale);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(indices).unwrap();
        }
        bytes
    }

    fn png_palette(color: [u8; 3]) -> Vec<u8> {
        let mut rgba = vec![0u8; 256 * 4];
        for entry in rgba.chunks_exact_mut(4) {
            entry[..3].copy_from_slice(&color);
            entry[3] = 255;
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, 256, 1);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        bytes
    }

    fn write_package(root: &Path, id: &str, tags: &str, color: [u8; 3]) {
        write_package_with_animation(root, id, tags, color, None);
    }

    fn write_package_with_animation(
        root: &Path,
        id: &str,
        tags: &str,
        color: [u8; 3],
        animation: Option<&str>,
    ) {
        let directory = root.join(id);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("scene.png"), png_scene(&[0, 255])).unwrap();
        std::fs::write(directory.join("palette.png"), png_palette(color)).unwrap();
        let animation_field = animation.map_or(String::new(), |_| {
            "animation = \"animation.toml\"\n".to_owned()
        });
        std::fs::write(
            directory.join("scene.toml"),
            format!(
                "schema_version = 1\nid = \"{id}\"\nimage = \"scene.png\"\npalette = \"palette.png\"\n{animation_field}{tags}"
            ),
        )
        .unwrap();
        if let Some(animation) = animation {
            std::fs::write(directory.join("animation.toml"), animation).unwrap();
        }
    }

    fn write_adaptive_package(
        root: &Path,
        id: &str,
        family: &str,
        band: TemperatureBand,
        weather: Weather,
    ) {
        let directory = root.join(id);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("scene.png"), png_scene(&[0, 255])).unwrap();
        std::fs::write(directory.join("palette.png"), png_palette([20, 30, 40])).unwrap();
        std::fs::write(
            directory.join("scene.toml"),
            format!(
                "schema_version = 2\nid = \"{id}\"\nimage = \"scene.png\"\npalette = \"palette.png\"\nfamily = \"{family}\"\ntemperature_band = \"{}\"\nweather = [\"{}\"]\n",
                band.as_str(),
                weather_name(weather),
            ),
        )
        .unwrap();
    }

    fn weather_name(weather: Weather) -> &'static str {
        match weather {
            Weather::Clear => "clear",
            Weather::Cloudy => "cloudy",
            Weather::Rain => "rain",
            Weather::Snow => "snow",
            _ => panic!("unsupported adaptive weather fixture"),
        }
    }

    fn adaptive_registry() -> (PathBuf, SceneRegistry) {
        let root = package_root("adaptive");
        std::fs::create_dir_all(&root).unwrap();
        let mut ids = Vec::new();
        let families = [
            ("v26-winter-forest", TemperatureBand::Cold),
            ("v19-mountain-stream", TemperatureBand::Cool),
            ("v16-mirror-pond", TemperatureBand::Cool),
            ("v30-deep-forest", TemperatureBand::Mild),
            ("vw3b-monolith-plains", TemperatureBand::Mild),
            ("v20-crystal-caves", TemperatureBand::Mild),
            ("v08-jungle-waterfall", TemperatureBand::Warm),
            ("v29-seascape", TemperatureBand::Warm),
            ("coral-aquarius", TemperatureBand::Warm),
            ("v25-desert", TemperatureBand::Hot),
        ];
        for (family, band) in families {
            let variants: &[Weather] = match family {
                "v26-winter-forest" => &[Weather::Clear, Weather::Snow],
                "v19-mountain-stream" | "v08-jungle-waterfall" | "v29-seascape" | "v25-desert" => {
                    &[Weather::Clear, Weather::Cloudy]
                }
                "v30-deep-forest" | "v16-mirror-pond" => &[Weather::Clear, Weather::Rain],
                _ => &[Weather::Clear],
            };
            for weather in variants {
                let id = if *weather == Weather::Clear {
                    family.to_owned()
                } else {
                    format!("{family}--{}", weather_name(*weather))
                };
                write_adaptive_package(&root, &id, family, band, *weather);
                ids.push(id);
            }
        }
        let scene_ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
        write_registry(&root, "v29-seascape", &scene_ids);
        let registry = SceneRegistry::load(&root).unwrap();
        (root, registry)
    }

    fn write_registry(root: &Path, fallback: &str, scenes: &[&str]) {
        let list = scenes
            .iter()
            .map(|id| format!("\"{id}\""))
            .collect::<Vec<_>>()
            .join(", ");
        std::fs::write(
            root.join("registry.toml"),
            format!("schema_version = 1\nfallback = \"{fallback}\"\nscenes = [{list}]\n"),
        )
        .unwrap();
    }

    const ANIMATED_HUE: &str = r#"schema_version = 1

[[group]]
name = "hue"
effect = "hue_cycle"
range = [1, 2]
speed = 0.25
amplitude = 0.5
phase = 0.0
"#;

    const ANIMATED_PULSE: &str = r#"schema_version = 1

[[group]]
name = "pulse"
effect = "brightness_pulse"
range = [3, 4]
speed = 0.5
amplitude = 0.25
phase = 0.25
"#;

    fn loaded_registry() -> (PathBuf, SceneRegistry) {
        let root = package_root("registry");
        std::fs::create_dir_all(&root).unwrap();
        write_package(&root, "day", "times = [\"day\"]\n", [20, 30, 40]);
        write_package(&root, "night", "times = [\"night\"]\n", [50, 60, 70]);
        write_registry(&root, "day", &["day", "night"]);
        let registry = SceneRegistry::load(&root).unwrap();
        (root, registry)
    }

    #[test]
    fn registry_loads_two_valid_packages_and_rejects_bad_fallback() {
        let (root, registry) = loaded_registry();
        assert_eq!(registry.len(), 2);
        assert_eq!(registry.fallback_id(), Some("day"));
        write_registry(&root, "missing", &["day", "night"]);
        let error = SceneRegistry::load(&root).unwrap_err();
        assert!(error.to_string().contains("fallback"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multiple_animated_packages_load_without_renderer_specific_selection() {
        let root = package_root("animated");
        std::fs::create_dir_all(&root).unwrap();
        write_package_with_animation(
            &root,
            "hue-scene",
            "times = [\"day\"]\n",
            [20, 30, 40],
            Some(ANIMATED_HUE),
        );
        write_package_with_animation(
            &root,
            "pulse-scene",
            "times = [\"night\"]\n",
            [50, 60, 70],
            Some(ANIMATED_PULSE),
        );
        write_registry(&root, "hue-scene", &["hue-scene", "pulse-scene"]);

        let registry = SceneRegistry::load(&root).unwrap();
        let hue = registry.get("hue-scene").unwrap().load_scene().unwrap();
        let pulse = registry.get("pulse-scene").unwrap().load_scene().unwrap();
        assert_eq!(hue.scene.animation.groups.len(), 1);
        assert_eq!(pulse.scene.animation.groups.len(), 1);
        assert_ne!(
            hue.scene.animation.groups[0].effect,
            pulse.scene.animation.groups[0].effect
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_rejects_escape_duplicate_tags_and_unknown_values() {
        let root = package_root("invalid");
        std::fs::create_dir_all(root.join("scene")).unwrap();
        std::fs::write(root.join("outside.png"), png_scene(&[0])).unwrap();
        std::fs::write(
            root.join("scene/scene.toml"),
            "schema_version = 1\nid = \"scene\"\nimage = \"../outside.png\"\npalette = \"palette.png\"\n",
        )
        .unwrap();
        let error = SceneManifest::load(root.join("scene/scene.toml")).unwrap_err();
        assert!(error.to_string().contains("inside") || error.to_string().contains("palette"));
        std::fs::remove_dir_all(root).unwrap();

        let root = package_root("tags");
        std::fs::create_dir_all(&root).unwrap();
        write_package(
            &root,
            "scene",
            "times = [\"day\", \"day\"]\nweather = [\"hail\"]\n",
            [1, 2, 3],
        );
        write_registry(&root, "scene", &["scene"]);
        let error = SceneRegistry::load(&root);
        assert!(error.is_err());
        let _ = std::fs::remove_dir_all(root);

        let root = package_root("version");
        std::fs::create_dir_all(&root).unwrap();
        write_package(&root, "scene", "", [1, 2, 3]);
        std::fs::write(
            root.join("scene/scene.toml"),
            "schema_version = 99\nid = \"scene\"\nimage = \"scene.png\"\npalette = \"palette.png\"\n",
        )
        .unwrap();
        write_registry(&root, "scene", &["scene"]);
        let error = SceneRegistry::load(&root).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported scene schema_version")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selector_is_repeatable_scores_specific_tags_and_ignores_non_triggers() {
        let (root, registry) = loaded_registry();
        let snapshot = EnvironmentSnapshot {
            time: TimeOfDay::Night,
            weather: Weather::Unknown,
            season: Season::Unknown,
        };
        let first = registry.select(snapshot, 42, SelectionTrigger::Reload, None);
        let second = registry.select(snapshot, 42, SelectionTrigger::Reload, None);
        assert_eq!(first, second);
        match first {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "night"),
            other => panic!("unexpected selection: {other:?}"),
        }
        assert_eq!(
            registry.select(snapshot, 42, SelectionTrigger::WeatherUpdated, None),
            SelectionResult::Ignored
        );
        match registry.select(snapshot, 42, SelectionTrigger::ManualId("day".into()), None) {
            SelectionResult::Selected(decision) => {
                assert_eq!(decision.id, "day");
                assert_eq!(decision.match_score, None);
            }
            other => panic!("unexpected manual selection: {other:?}"),
        }
        assert!(matches!(
            registry.select(
                snapshot,
                42,
                SelectionTrigger::ManualId("missing".into()),
                None
            ),
            SelectionResult::Rejected(_)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn environment_snapshot_changes_are_inert_until_an_explicit_trigger() {
        let (root, registry) = loaded_registry();
        let mut controller = SelectionController::new(registry);
        let initial = EnvironmentSnapshot::unknown();
        let startup = controller
            .request(initial, None, 1, SelectionTrigger::Startup)
            .unwrap();
        controller.mark_active(&startup.id).unwrap();
        let changed = EnvironmentSnapshot {
            time: TimeOfDay::Night,
            weather: Weather::Clear,
            season: Season::Winter,
        };
        assert!(matches!(
            controller.request(changed, None, 2, SelectionTrigger::ClockTick),
            Err(SelectionResult::Ignored)
        ));
        assert_eq!(controller.generation(), 1);
        controller.invalidate_requests();
        assert_eq!(controller.generation(), 1);
        let reload = controller
            .request(changed, None, 2, SelectionTrigger::Reload)
            .unwrap();
        assert_eq!(reload.generation, 3);
        assert_eq!(controller.generation(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_values_need_explicit_unknown_or_wildcard_and_fallback_is_used() {
        let (root, registry) = loaded_registry();
        let unknown = EnvironmentSnapshot::unknown();
        match registry.select(unknown, 1, SelectionTrigger::Startup, None) {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "day"),
            other => panic!("unexpected selection: {other:?}"),
        }
        let empty = SceneRegistry::empty();
        assert!(matches!(
            empty.select(unknown, 1, SelectionTrigger::Startup, None),
            SelectionResult::NoScene(_)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn next_excludes_active_when_alternative_exists_and_empty_is_explicitly_safe() {
        let (root, registry) = loaded_registry();
        let mut controller = SelectionController::new(registry);
        controller.mark_active("day").unwrap();
        let request = controller
            .request(
                EnvironmentSnapshot {
                    time: TimeOfDay::Night,
                    weather: Weather::Unknown,
                    season: Season::Unknown,
                },
                None,
                0,
                SelectionTrigger::Next,
            )
            .unwrap();
        assert_eq!(request.id, "night");
        assert_eq!(controller.generation(), 1);
        for trigger in [
            SelectionTrigger::WeatherUpdated,
            SelectionTrigger::ClockTick,
            SelectionTrigger::OutputChanged,
            SelectionTrigger::ConfigChanged,
        ] {
            assert!(matches!(
                controller.request(EnvironmentSnapshot::unknown(), None, 0, trigger),
                Err(SelectionResult::Ignored)
            ));
        }
        assert_eq!(controller.generation(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transaction_rejects_stale_and_failed_candidates_without_losing_active_scene() {
        let (root, registry) = loaded_registry();
        let day = registry.get("day").unwrap().load_scene().unwrap();
        let night = registry.get("night").unwrap().load_scene().unwrap();
        let mut transaction = SceneTransaction::default();
        let first = SelectionRequest {
            generation: 1,
            id: "day".into(),
            trigger: SelectionTrigger::Startup,
            rationale: "test".into(),
        };
        let first_token = transaction.begin(&first);
        assert!(matches!(
            transaction.complete(first_token, Ok(day)),
            CommitOutcome::Applied { .. }
        ));
        let second = SelectionRequest {
            generation: 2,
            id: "night".into(),
            trigger: SelectionTrigger::Reload,
            rationale: "test".into(),
        };
        let second_token = transaction.begin(&second);
        let stale = transaction.begin(&SelectionRequest {
            generation: 3,
            id: "day".into(),
            trigger: SelectionTrigger::ManualId("day".into()),
            rationale: "test".into(),
        });
        assert!(matches!(
            transaction.complete(second_token, Ok(night)),
            CommitOutcome::Stale { generation: 2 }
        ));
        let retained_key = transaction.active().unwrap().content_key;
        assert!(matches!(
            transaction.complete(stale, Err(error("corrupt candidate"))),
            CommitOutcome::Failed {
                retained_id: Some(ref id),
                ..
            } if id == "day"
        ));
        assert_eq!(transaction.active().unwrap().content_key, retained_key);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unchanged_content_skips_replacement() {
        let (root, registry) = loaded_registry();
        let day = registry.get("day").unwrap().load_scene().unwrap();
        let same = registry.get("day").unwrap().load_scene().unwrap();
        let mut transaction = SceneTransaction::default();
        let token = transaction.begin(&SelectionRequest {
            generation: 1,
            id: "day".into(),
            trigger: SelectionTrigger::Startup,
            rationale: "test".into(),
        });
        assert!(matches!(
            transaction.complete(token, Ok(day)),
            CommitOutcome::Applied { .. }
        ));
        let token = transaction.begin(&SelectionRequest {
            generation: 2,
            id: "day".into(),
            trigger: SelectionTrigger::Reload,
            rationale: "test".into(),
        });
        let mut swap_calls = 0;
        assert!(matches!(
            transaction.complete_with_swap(token, Ok(same), |_| {
                swap_calls += 1;
                Ok::<(), &'static str>(())
            }),
            CommitOutcome::Unchanged { .. }
        ));
        assert_eq!(swap_calls, 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn renderer_swap_failure_retains_active_candidate() {
        let (root, registry) = loaded_registry();
        let day = registry.get("day").unwrap().load_scene().unwrap();
        let night = registry.get("night").unwrap().load_scene().unwrap();
        let mut transaction = SceneTransaction::default();
        let token = transaction.begin(&SelectionRequest {
            generation: 1,
            id: "day".into(),
            trigger: SelectionTrigger::Startup,
            rationale: "test".into(),
        });
        assert!(matches!(
            transaction.complete(token, Ok(day)),
            CommitOutcome::Applied { .. }
        ));
        let retained_key = transaction.active().unwrap().content_key;
        let token = transaction.begin(&SelectionRequest {
            generation: 2,
            id: "night".into(),
            trigger: SelectionTrigger::ManualId("night".into()),
            rationale: "test".into(),
        });
        let outcome = transaction.complete_with_swap(token, Ok(night), |_| {
            Err::<(), _>("simulated renderer upload failure")
        });
        assert!(matches!(
            outcome,
            CommitOutcome::Failed {
                id,
                retained_id: Some(ref retained),
                ref reason,
            } if id == "night"
                && retained == "day"
                && reason.contains("renderer scene swap failed")
        ));
        assert_eq!(transaction.active().unwrap().id, "day");
        assert_eq!(transaction.active().unwrap().content_key, retained_key);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_completion_never_reaches_renderer_swap_callback() {
        let (root, registry) = loaded_registry();
        let day = registry.get("day").unwrap().load_scene().unwrap();
        let night = registry.get("night").unwrap().load_scene().unwrap();
        let mut transaction = SceneTransaction::default();
        let first = transaction.begin(&SelectionRequest {
            generation: 1,
            id: "day".into(),
            trigger: SelectionTrigger::Startup,
            rationale: "test".into(),
        });
        assert!(matches!(
            transaction.complete(first, Ok(day)),
            CommitOutcome::Applied { .. }
        ));
        let stale = transaction.begin(&SelectionRequest {
            generation: 2,
            id: "night".into(),
            trigger: SelectionTrigger::Reload,
            rationale: "test".into(),
        });
        let current = transaction.begin(&SelectionRequest {
            generation: 3,
            id: "day".into(),
            trigger: SelectionTrigger::ManualId("day".into()),
            rationale: "test".into(),
        });
        let mut swap_calls = 0;
        assert!(matches!(
            transaction.complete_with_swap(stale, Ok(night), |_| {
                swap_calls += 1;
                Ok::<(), &'static str>(())
            }),
            CommitOutcome::Stale { generation: 2 }
        ));
        assert_eq!(swap_calls, 0);
        assert_eq!(current.generation, 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scene_load_worker_decodes_and_transactions_reject_invalidated_work() {
        let (root, registry) = loaded_registry();
        let manifest = registry.get("day").unwrap().clone();
        let request = SelectionRequest {
            generation: 1,
            id: "day".into(),
            trigger: SelectionTrigger::WeatherUpdated,
            rationale: "same-family variant test".into(),
        };
        let token = SceneLoadToken {
            generation: request.generation,
            id: request.id.clone(),
        };
        let worker = SceneLoadWorker::new().unwrap();
        worker
            .submit(SceneLoadJob {
                request,
                token,
                manifest,
                resulting_state: Some(AdaptiveSelectionState {
                    family_id: "test-family".into(),
                    temperature_band: TemperatureBand::Mild,
                }),
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let completion = loop {
            if let Some(completion) = worker.try_recv() {
                break completion;
            }
            assert!(Instant::now() < deadline, "scene-load worker timed out");
            std::thread::yield_now();
        };
        assert_eq!(completion.candidate.unwrap().id, "day");
        assert_eq!(
            completion.resulting_state,
            Some(AdaptiveSelectionState {
                family_id: "test-family".into(),
                temperature_band: TemperatureBand::Mild,
            })
        );

        let mut transaction = SceneTransaction::default();
        let request = SelectionRequest {
            generation: 2,
            id: "day".into(),
            trigger: SelectionTrigger::WeatherUpdated,
            rationale: "queued completion".into(),
        };
        let token = transaction.begin(&request);
        transaction.invalidate_through(3);
        assert!(matches!(
            transaction.complete(token, registry.get("day").unwrap().load_scene()),
            CommitOutcome::Stale { generation: 2 }
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scene_load_worker_queue_is_bounded_and_decode_errors_are_reported() {
        use calloop::{
            EventLoop,
            timer::{TimeoutAction, Timer},
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = Arc::clone(&calls);
        let mut worker = SceneLoadWorker::with_loader(move |_| {
            if worker_calls.fetch_add(1, Ordering::AcqRel) == 0 {
                started_sender.send(()).unwrap();
                release_receiver.recv().unwrap();
            }
            Err(error("synthetic scene decode failure"))
        })
        .unwrap();
        let job = |generation| {
            let request = SelectionRequest {
                generation,
                id: "blocked-scene".into(),
                trigger: SelectionTrigger::ManualId("blocked-scene".into()),
                rationale: "bounded worker test".into(),
            };
            SceneLoadJob {
                token: SceneLoadToken {
                    generation,
                    id: request.id.clone(),
                },
                request,
                manifest: SceneManifest {
                    schema_version: 2,
                    id: "blocked-scene".into(),
                    package_dir: PathBuf::new(),
                    image_path: PathBuf::new(),
                    palette_path: PathBuf::new(),
                    animation_path: None,
                    time_of_day_path: None,
                    times: Vec::new(),
                    weather: vec![Weather::Clear],
                    seasons: Vec::new(),
                    family_id: None,
                    temperature_band: None,
                },
                resulting_state: None,
            }
        };

        let mut event_loop: EventLoop<usize> = EventLoop::try_new().unwrap();
        let wakeups = worker.take_wakeup_channel().unwrap();
        event_loop
            .handle()
            .insert_source(wakeups, |event, _, count| {
                if matches!(event, calloop::channel::Event::Msg(())) {
                    *count += 1;
                }
            })
            .unwrap();
        event_loop
            .handle()
            .insert_source(
                Timer::from_duration(Duration::from_millis(1)),
                |_, _, count| {
                    *count += 10;
                    TimeoutAction::Drop
                },
            )
            .unwrap();

        worker.submit(job(1)).unwrap();
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        worker.submit(job(2)).unwrap();
        assert!(worker.submit(job(3)).is_err());
        let mut dispatched = 0;
        event_loop
            .dispatch(Some(Duration::from_secs(1)), &mut dispatched)
            .unwrap();
        assert_eq!(dispatched, 10, "owner timer should run during slow decode");
        assert!(worker.try_recv().is_none());
        release_sender.send(()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);
        let first = loop {
            event_loop
                .dispatch(Some(Duration::from_millis(50)), &mut dispatched)
                .unwrap();
            if let Some(completion) = worker.try_recv() {
                break completion;
            }
            assert!(
                Instant::now() < deadline,
                "scene worker completion timed out"
            );
        };
        assert_eq!(first.request.generation, 1);
        assert!(first.candidate.is_err());
        let deadline = Instant::now() + Duration::from_secs(1);
        let second = loop {
            event_loop
                .dispatch(Some(Duration::from_millis(50)), &mut dispatched)
                .unwrap();
            if let Some(completion) = worker.try_recv() {
                break completion;
            }
            assert!(
                Instant::now() < deadline,
                "queued scene worker completion timed out"
            );
        };
        assert_eq!(second.request.generation, 2);
        assert!(second.candidate.is_err());
    }

    #[test]
    fn temperature_bands_follow_approved_thresholds_and_hysteresis() {
        assert_eq!(TemperatureBand::classify(0.0), Some(TemperatureBand::Cold));
        assert_eq!(TemperatureBand::classify(0.1), Some(TemperatureBand::Cool));
        assert_eq!(TemperatureBand::classify(10.0), Some(TemperatureBand::Cool));
        assert_eq!(TemperatureBand::classify(10.1), Some(TemperatureBand::Mild));
        assert_eq!(TemperatureBand::classify(20.0), Some(TemperatureBand::Mild));
        assert_eq!(TemperatureBand::classify(20.1), Some(TemperatureBand::Warm));
        assert_eq!(TemperatureBand::classify(28.0), Some(TemperatureBand::Warm));
        assert_eq!(TemperatureBand::classify(28.1), Some(TemperatureBand::Hot));
        assert_eq!(TemperatureBand::classify(f32::NAN), None);
        assert_eq!(TemperatureBand::classify(101.0), None);

        assert_eq!(
            TemperatureBand::with_hysteresis(2.0, Some(TemperatureBand::Cold)),
            Some(TemperatureBand::Cold)
        );
        assert_eq!(
            TemperatureBand::with_hysteresis(2.01, Some(TemperatureBand::Cold)),
            Some(TemperatureBand::Cool)
        );
        assert_eq!(
            TemperatureBand::with_hysteresis(-1.99, Some(TemperatureBand::Cool)),
            Some(TemperatureBand::Cool)
        );
        assert_eq!(
            TemperatureBand::with_hysteresis(-2.0, Some(TemperatureBand::Cool)),
            Some(TemperatureBand::Cold)
        );
        assert_eq!(
            TemperatureBand::with_hysteresis(12.0, Some(TemperatureBand::Cool)),
            Some(TemperatureBand::Cool)
        );
        assert_eq!(
            TemperatureBand::with_hysteresis(12.01, Some(TemperatureBand::Cool)),
            Some(TemperatureBand::Mild)
        );
        assert_eq!(
            TemperatureBand::with_hysteresis(18.0, Some(TemperatureBand::Warm)),
            Some(TemperatureBand::Mild)
        );
        assert_eq!(
            TemperatureBand::with_hysteresis(31.0, Some(TemperatureBand::Cold)),
            Some(TemperatureBand::Hot)
        );
    }

    #[test]
    fn adaptive_registry_selects_by_exact_variant_then_stable_family_and_holds_hysteresis() {
        let (root, registry) = adaptive_registry();
        registry.validate_adaptive().unwrap();

        let cool = registry
            .adaptive_full_selection(Some(8.0), PrecipitationAppearance::Cloudy, None, None, None)
            .unwrap();
        assert_eq!(cool.scene_id, "v19-mountain-stream--cloudy");
        assert_eq!(cool.state.family_id, "v19-mountain-stream");
        assert_eq!(cool.state.temperature_band, TemperatureBand::Cool);

        let mild = registry
            .adaptive_full_selection(Some(19.0), PrecipitationAppearance::Rain, None, None, None)
            .unwrap();
        assert_eq!(mild.scene_id, "v30-deep-forest--rain");
        assert_eq!(mild.state.temperature_band, TemperatureBand::Mild);

        let held = registry
            .adaptive_full_selection(
                Some(21.0),
                PrecipitationAppearance::Clear,
                Some(&mild.state),
                Some("v30-deep-forest"),
                None,
            )
            .unwrap();
        assert_eq!(held.state.family_id, "v30-deep-forest");
        assert_eq!(held.state.temperature_band, TemperatureBand::Mild);
        assert_eq!(held.scene_id, "v30-deep-forest");

        let changed = registry
            .adaptive_full_selection(
                Some(22.01),
                PrecipitationAppearance::Clear,
                Some(&mild.state),
                Some("v30-deep-forest"),
                None,
            )
            .unwrap();
        assert_eq!(changed.state.temperature_band, TemperatureBand::Warm);
        assert_eq!(changed.state.family_id, "coral-aquarius");
        assert_eq!(changed.scene_id, "coral-aquarius");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adaptive_unknown_data_retains_active_variant_or_uses_v29_offline_fallback() {
        let (root, registry) = adaptive_registry();
        let retained = registry
            .adaptive_full_selection(
                None,
                PrecipitationAppearance::Unknown,
                Some(&AdaptiveSelectionState {
                    family_id: "v29-seascape".into(),
                    temperature_band: TemperatureBand::Warm,
                }),
                Some("v29-seascape--cloudy"),
                None,
            )
            .unwrap();
        assert_eq!(retained.scene_id, "v29-seascape--cloudy");
        assert_eq!(retained.state.family_id, "v29-seascape");

        let fallback = registry
            .adaptive_full_selection(None, PrecipitationAppearance::Unknown, None, None, None)
            .unwrap();
        assert_eq!(fallback.scene_id, "v29-seascape");
        assert_eq!(fallback.state.temperature_band, TemperatureBand::Warm);

        let restored = registry
            .adaptive_full_selection(
                None,
                PrecipitationAppearance::Clear,
                Some(&AdaptiveSelectionState {
                    family_id: "missing-family".into(),
                    temperature_band: TemperatureBand::Cold,
                }),
                None,
                None,
            )
            .unwrap();
        assert_eq!(restored.scene_id, "v29-seascape");
        assert_eq!(restored.state.family_id, "v29-seascape");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn weather_variant_selection_is_strictly_same_family_and_uses_fallback_order() {
        let (root, registry) = adaptive_registry();
        let snow = registry
            .adaptive_variant_in_family("v26-winter-forest", PrecipitationAppearance::Snow, None)
            .unwrap();
        assert_eq!(snow.id, "v26-winter-forest--snow");
        let rain_without_rain_variant = registry
            .adaptive_variant_in_family("v29-seascape", PrecipitationAppearance::Rain, None)
            .unwrap();
        assert_eq!(rain_without_rain_variant.id, "v29-seascape--cloudy");
        let unknown = registry.adaptive_variant_in_family(
            "v29-seascape",
            PrecipitationAppearance::Unknown,
            None,
        );
        assert!(unknown.is_none());
        assert!(
            registry
                .adaptive_variant_in_family("missing-family", PrecipitationAppearance::Clear, None)
                .is_none()
        );
        assert_eq!(
            PrecipitationAppearance::from_weather(Weather::Storm, true),
            PrecipitationAppearance::Rain
        );
        assert_eq!(
            PrecipitationAppearance::from_weather(Weather::Storm, false),
            PrecipitationAppearance::Unknown
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn schema_v2_scene_requires_complete_family_metadata_and_v1_stays_unchanged() {
        let root = package_root("scene-schema2");
        std::fs::create_dir_all(&root).unwrap();
        write_package(&root, "legacy", "weather = [\"clear\"]\n", [1, 2, 3]);
        let legacy = SceneManifest::load(root.join("legacy/scene.toml"))
            .unwrap()
            .0;
        assert_eq!(legacy.schema_version, 1);
        assert_eq!(legacy.family_id, None);

        write_package(&root, "incomplete", "weather = [\"clear\"]\n", [1, 2, 3]);
        let path = root.join("incomplete/scene.toml");
        let contents = std::fs::read_to_string(&path)
            .unwrap()
            .replace("schema_version = 1", "schema_version = 2");
        std::fs::write(path, contents).unwrap();
        let error = SceneManifest::load(root.join("incomplete/scene.toml")).unwrap_err();
        assert!(error.to_string().contains("requires a safe family ID"));
        std::fs::remove_dir_all(root).unwrap();
    }

    // -----------------------------------------------------------------------
    // S12-AC5: the shared eligibility gate over every automatic path.

    use crate::scene_pools::{PoolEntry, PoolSource, ScenePools, SceneRule};

    fn pool_state(scene_ids: &[&str], disabled: &[&str]) -> ScenePools {
        let rules = disabled
            .iter()
            .map(|id| {
                (
                    (*id).to_owned(),
                    SceneRule {
                        enabled: false,
                        ..SceneRule::default()
                    },
                )
            })
            .collect::<Vec<_>>();
        ScenePools::from_parts(
            vec![PoolEntry {
                id: "test-pool".into(),
                source: PoolSource::Registry,
                enabled: true,
                scene_ids: scene_ids.iter().map(|id| (*id).to_owned()).collect(),
            }],
            rules,
        )
        .unwrap()
    }

    fn cycling_registry() -> (PathBuf, SceneRegistry) {
        let root = package_root("cycling");
        std::fs::create_dir_all(&root).unwrap();
        for id in ["alpha", "beta", "gamma"] {
            write_package(&root, id, "times = [\"day\", \"night\"]\n", [1, 2, 3]);
        }
        write_registry(&root, "alpha", &["alpha", "beta", "gamma"]);
        let registry = SceneRegistry::load(&root).unwrap();
        (root, registry)
    }

    fn tagged_registry() -> (PathBuf, SceneRegistry) {
        let root = package_root("tagged");
        std::fs::create_dir_all(&root).unwrap();
        write_package(
            &root,
            "day",
            "times = [\"day\"]\nweather = [\"clear\"]\nseasons = [\"summer\"]\n",
            [20, 30, 40],
        );
        write_package(
            &root,
            "night",
            "times = [\"night\"]\nweather = [\"rain\"]\nseasons = [\"winter\"]\n",
            [50, 60, 70],
        );
        write_registry(&root, "day", &["day", "night"]);
        let registry = SceneRegistry::load(&root).unwrap();
        (root, registry)
    }

    const ADAPTIVE_POOL_SCENES: &[&str] = &[
        "v26-winter-forest",
        "v26-winter-forest--snow",
        "v19-mountain-stream",
        "v19-mountain-stream--cloudy",
        "v16-mirror-pond",
        "v16-mirror-pond--rain",
        "v30-deep-forest",
        "v30-deep-forest--rain",
        "vw3b-monolith-plains",
        "v20-crystal-caves",
        "v08-jungle-waterfall",
        "v08-jungle-waterfall--cloudy",
        "v29-seascape",
        "v29-seascape--cloudy",
        "coral-aquarius",
        "v25-desert",
        "v25-desert--cloudy",
    ];

    #[test]
    fn pool_gate_constrains_automatic_paths_and_manual_id_is_an_override() {
        let (root, registry) = cycling_registry();
        let pools = pool_state(&["alpha", "beta", "gamma"], &["beta"]);
        let snapshot = EnvironmentSnapshot {
            time: TimeOfDay::Day,
            weather: Weather::Clear,
            season: Season::Summer,
        };
        let environment = PoolEnvironment::new(&snapshot, None);
        let gate = PoolGate {
            pools: &pools,
            environment: &environment,
        };

        // Full selection sees only eligible candidates.
        match registry.select(snapshot, 42, SelectionTrigger::Reload, Some(&gate)) {
            SelectionResult::Selected(decision) => assert_ne!(
                decision.id, "beta",
                "the disabled scene must never be selected"
            ),
            other => panic!("unexpected selection: {other:?}"),
        }
        // Manual IDs are the explicit override and still load known packages.
        match registry.select(
            snapshot,
            42,
            SelectionTrigger::ManualId("beta".into()),
            Some(&gate),
        ) {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "beta"),
            other => panic!("unexpected manual selection: {other:?}"),
        }
        // Clock/weather/config triggers stay inert with or without the gate.
        for trigger in [
            SelectionTrigger::WeatherUpdated,
            SelectionTrigger::ClockTick,
            SelectionTrigger::OutputChanged,
            SelectionTrigger::ConfigChanged,
        ] {
            assert_eq!(
                registry.select(snapshot, 42, trigger.clone(), Some(&gate)),
                SelectionResult::Ignored
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gated_next_cycles_eligible_entries_with_wrap_and_no_op_reasons() {
        let (root, registry) = cycling_registry();
        let snapshot = EnvironmentSnapshot {
            time: TimeOfDay::Day,
            weather: Weather::Clear,
            season: Season::Summer,
        };

        // Deterministic ID order with wrap over the eligible set.
        let pools = pool_state(&["alpha", "beta", "gamma"], &[]);
        let mut controller = SelectionController::new(registry);
        controller.set_pools(Some(pools));
        controller.mark_active("alpha").unwrap();
        let first = controller
            .request(snapshot, None, 7, SelectionTrigger::Next)
            .unwrap();
        assert_eq!(first.id, "beta");
        controller.mark_active("beta").unwrap();
        let second = controller
            .request(snapshot, None, 7, SelectionTrigger::Next)
            .unwrap();
        assert_eq!(second.id, "gamma");
        controller.mark_active("gamma").unwrap();
        let wrapped = controller
            .request(snapshot, None, 7, SelectionTrigger::Next)
            .unwrap();
        assert_eq!(wrapped.id, "alpha");

        // Disabled scenes are skipped, never cycled to.
        controller.set_pools(Some(pool_state(&["alpha", "beta", "gamma"], &["beta"])));
        controller.mark_active("alpha").unwrap();
        let skipped = controller
            .request(snapshot, None, 7, SelectionTrigger::Next)
            .unwrap();
        assert_eq!(skipped.id, "gamma");

        // With no eligible candidate, `next` is a no-op with a bounded reason.
        controller.set_pools(Some(pool_state(
            &["alpha", "beta", "gamma"],
            &["alpha", "beta", "gamma"],
        )));
        match controller.request(snapshot, None, 7, SelectionTrigger::Next) {
            Err(SelectionResult::Noop(reason)) => {
                assert_eq!(reason, "no eligible scene for next")
            }
            other => panic!("unexpected next result: {other:?}"),
        }
        // Startup with an empty eligible set reports no eligible scene.
        match controller.request(snapshot, None, 7, SelectionTrigger::Startup) {
            Err(SelectionResult::NoScene(reason)) => assert_eq!(reason, "no eligible scene"),
            other => panic!("unexpected startup result: {other:?}"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn designated_fallback_is_selectable_only_inside_the_eligible_set() {
        let (root, registry) = tagged_registry();
        let unknown = EnvironmentSnapshot::unknown();

        // Legacy behavior with no sidecar is unchanged: all-zero scores use
        // the designated fallback.
        match registry.select(unknown, 1, SelectionTrigger::Startup, None) {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "day"),
            other => panic!("unexpected legacy selection: {other:?}"),
        }

        // With pool rules the fallback is never a bypass: only the eligible
        // candidate is ranked.
        let pools = pool_state(&["day", "night"], &["day"]);
        let environment = PoolEnvironment::new(&unknown, None);
        let gate = PoolGate {
            pools: &pools,
            environment: &environment,
        };
        match registry.select(unknown, 1, SelectionTrigger::Startup, Some(&gate)) {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "night"),
            other => panic!("unexpected gated selection: {other:?}"),
        }

        // An empty eligible set fails closed instead of using the fallback.
        let pools = pool_state(&["day", "night"], &["day", "night"]);
        let gate = PoolGate {
            pools: &pools,
            environment: &environment,
        };
        match registry.select(unknown, 1, SelectionTrigger::Startup, Some(&gate)) {
            SelectionResult::NoScene(reason) => assert_eq!(reason, "no eligible scene"),
            other => panic!("unexpected empty-set result: {other:?}"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn temperature_criteria_need_known_values_and_match_exact_boundaries() {
        let (root, registry) = tagged_registry();
        let rules = vec![(
            "night".to_owned(),
            SceneRule {
                enabled: true,
                time_of_day: None,
                weather_any: None,
                temperature_min_c: Some(5.0),
                temperature_max_c: Some(10.0),
            },
        )];
        let pools = ScenePools::from_parts(
            vec![PoolEntry {
                id: "test-pool".into(),
                source: PoolSource::Registry,
                enabled: true,
                scene_ids: vec!["day".into(), "night".into()],
            }],
            rules,
        )
        .unwrap();
        let snapshot = EnvironmentSnapshot {
            time: TimeOfDay::Night,
            weather: Weather::Rain,
            season: Season::Winter,
        };
        for (temperature, expected) in [(Some(5.0), "night"), (Some(10.0), "night")] {
            let environment = PoolEnvironment::new(&snapshot, temperature);
            let gate = PoolGate {
                pools: &pools,
                environment: &environment,
            };
            match registry.select(snapshot, 3, SelectionTrigger::Startup, Some(&gate)) {
                SelectionResult::Selected(decision) => assert_eq!(decision.id, expected),
                other => panic!("unexpected selection: {other:?}"),
            }
        }
        // An unknown required temperature does not match the criterion, so
        // only the unconstrained candidate remains.
        let environment = PoolEnvironment::new(&snapshot, None);
        let gate = PoolGate {
            pools: &pools,
            environment: &environment,
        };
        match registry.select(snapshot, 3, SelectionTrigger::Startup, Some(&gate)) {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "day"),
            other => panic!("unexpected unknown-temperature selection: {other:?}"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adaptive_variant_filter_walks_preference_order_without_crossing_families() {
        let (root, registry) = adaptive_registry();
        // Rain falls back rain -> cloudy -> clear within the family.
        assert_eq!(
            registry
                .adaptive_variant_in_family("v29-seascape", PrecipitationAppearance::Rain, None)
                .unwrap()
                .id,
            "v29-seascape--cloudy"
        );
        // The shared filter skips the preferred variant and continues in the
        // approved same-family order instead of crossing families.
        let pools = pool_state(ADAPTIVE_POOL_SCENES, &["v29-seascape--cloudy"]);
        let environment = PoolEnvironment::new(&EnvironmentSnapshot::unknown(), None);
        let gate = PoolGate {
            pools: &pools,
            environment: &environment,
        };
        assert_eq!(
            registry
                .adaptive_variant_in_family(
                    "v29-seascape",
                    PrecipitationAppearance::Rain,
                    Some(&gate)
                )
                .unwrap()
                .id,
            "v29-seascape",
            "eligible clear fallback stays inside the family"
        );
        // With every variant filtered, there is no eligible alternative.
        let pools = pool_state(
            ADAPTIVE_POOL_SCENES,
            &["v29-seascape--cloudy", "v29-seascape"],
        );
        let gate = PoolGate {
            pools: &pools,
            environment: &environment,
        };
        assert!(
            registry
                .adaptive_variant_in_family(
                    "v29-seascape",
                    PrecipitationAppearance::Rain,
                    Some(&gate)
                )
                .is_none(),
            "no ineligible fallback and no cross-family variant"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adaptive_full_selection_fails_closed_without_crossing_family_or_using_v29() {
        let (root, registry) = adaptive_registry();
        let environment = PoolEnvironment::new(&EnvironmentSnapshot::unknown(), None);
        // Cool band resolves to v19-mountain-stream; if all of its variants
        // are filtered the decision reports no eligible candidate instead of
        // crossing bands or falling back to V29.
        let pools = pool_state(
            ADAPTIVE_POOL_SCENES,
            &["v19-mountain-stream--cloudy", "v19-mountain-stream"],
        );
        let gate = PoolGate {
            pools: &pools,
            environment: &environment,
        };
        let error = registry
            .adaptive_full_selection(
                Some(8.0),
                PrecipitationAppearance::Cloudy,
                None,
                None,
                Some(&gate),
            )
            .unwrap_err();
        assert_eq!(error.to_string(), "no eligible candidate");

        // The offline V29 fallback is only selectable inside the eligible set.
        let pools = pool_state(ADAPTIVE_POOL_SCENES, &["v29-seascape"]);
        let gate = PoolGate {
            pools: &pools,
            environment: &environment,
        };
        let error = registry
            .adaptive_full_selection(
                None,
                PrecipitationAppearance::Unknown,
                None,
                None,
                Some(&gate),
            )
            .unwrap_err();
        assert_eq!(error.to_string(), "no eligible candidate");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adaptive_mode_pool_packages_stay_manual_only() {
        let (root, registry) = loaded_registry();
        let (extra_root, _) = loaded_registry();
        let mut registry = registry;
        let extra = SceneManifest::load(extra_root.join("day/scene.toml"))
            .unwrap()
            .0;
        // A package ID conflict is always refused.
        assert!(registry.add_pool_packages([extra], false).is_err());
        let (pool_root, pool_registry) = loaded_registry();
        let mut merged = pool_registry;
        let mut renamed = SceneManifest::load(pool_root.join("night/scene.toml"))
            .unwrap()
            .0;
        renamed.id = "library-extra".into();
        merged.add_pool_packages([renamed], false).unwrap();
        // Manual selection may load it, but no automatic path can select it.
        assert!(merged.get("library-extra").is_some());
        let unknown = EnvironmentSnapshot::unknown();
        match merged.select(unknown, 5, SelectionTrigger::Reload, None) {
            SelectionResult::Selected(decision) => assert_ne!(decision.id, "library-extra"),
            other => panic!("unexpected selection: {other:?}"),
        }
        match merged.select(
            unknown,
            5,
            SelectionTrigger::ManualId("library-extra".into()),
            None,
        ) {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "library-extra"),
            other => panic!("unexpected manual selection: {other:?}"),
        }
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(extra_root).unwrap();
        std::fs::remove_dir_all(pool_root).unwrap();
    }

    #[test]
    fn absent_sidecar_preserves_legacy_next_and_startup_outcomes() {
        let (root, registry) = loaded_registry();
        let unknown = EnvironmentSnapshot::unknown();
        // Legacy empty-compatible `next` keeps its historical NoScene outcome.
        let mut controller = SelectionController::new(registry);
        controller.set_pools(None);
        match controller.request(unknown, None, 0, SelectionTrigger::Next) {
            Err(SelectionResult::NoScene(reason)) => {
                assert_eq!(reason, "no compatible scene for next")
            }
            other => panic!("unexpected legacy next result: {other:?}"),
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
