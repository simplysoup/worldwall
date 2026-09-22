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

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    fs::File,
    hash::{Hash, Hasher},
    io::Read,
    path::{Component, Path, PathBuf},
};

use crate::{
    animation::Animation,
    scene::{self, Scene},
};

pub use crate::environment::{EnvironmentSnapshot, Season, TimeOfDay, Weather};

pub const SCENE_SCHEMA_VERSION: u32 = 1;
pub const REGISTRY_SCHEMA_VERSION: u32 = 1;
const MAX_REGISTRY_BYTES: usize = 1024 * 1024;
const MAX_ANIMATION_BYTES: usize = 1024 * 1024;
const TIME_WEIGHT: u32 = 5;
const WEATHER_WEIGHT: u32 = 4;
const SEASON_WEIGHT: u32 = 2;

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
    pub times: Vec<TimeOfDay>,
    pub weather: Vec<Weather>,
    pub seasons: Vec<Season>,
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
        let scene = Scene::with_animation(
            scene::decode_index_png(&image_bytes).map_err(|err| error(err.to_string()))?,
            scene::decode_palette_png(&palette_bytes).map_err(|err| error(err.to_string()))?,
            animation,
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
        if schema_version != SCENE_SCHEMA_VERSION {
            return Err(error(format!(
                "unsupported scene schema_version {schema_version}; expected {SCENE_SCHEMA_VERSION}"
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
        reject_duplicates(&values.times, "time")?;
        reject_duplicates(&values.weather, "weather")?;
        reject_duplicates(&values.seasons, "season")?;
        Ok(Self {
            schema_version,
            id,
            package_dir,
            image_path,
            palette_path,
            animation_path,
            times: values.times,
            weather: values.weather,
            seasons: values.seasons,
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
pub struct SceneRegistry {
    scenes: BTreeMap<String, SceneManifest>,
    fallback: Option<String>,
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
        for manifest in manifests {
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
        Ok(Self { scenes, fallback })
    }

    /// Empty is useful as a pure selector input and is not accepted by the
    /// on-disk loader, which requires a validated offline fallback.
    pub fn empty() -> Self {
        Self {
            scenes: BTreeMap::new(),
            fallback: None,
        }
    }

    pub fn get(&self, id: &str) -> Option<&SceneManifest> {
        self.scenes.get(id)
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

    pub fn manifests(&self) -> impl Iterator<Item = &SceneManifest> {
        self.scenes.values()
    }

    pub fn select(
        &self,
        snapshot: EnvironmentSnapshot,
        seed: u64,
        trigger: SelectionTrigger,
    ) -> SelectionResult {
        match trigger {
            SelectionTrigger::ManualId(id) => {
                if self.scenes.contains_key(&id) {
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
            SelectionTrigger::Next => self.select_next(snapshot, None),
            SelectionTrigger::Startup | SelectionTrigger::Resume | SelectionTrigger::Reload => {
                self.select_scored(snapshot, seed, trigger)
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
    ) -> SelectionResult {
        let mut candidates = self
            .scenes
            .values()
            .map(|manifest| {
                let match_score = manifest.environmental_score(snapshot);
                let random_value = deterministic_unit(seed, &manifest.id);
                (manifest, match_score, random_value)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
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
            let fallback = self.fallback.as_ref().expect("validated registry fallback");
            return SelectionResult::Selected(SelectionDecision {
                id: fallback.clone(),
                trigger,
                match_score: None,
                random_value: None,
                rationale: "designated fallback; no environmental tag matched".into(),
            });
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
    ) -> SelectionResult {
        let compatible = self
            .scenes
            .values()
            .filter(|manifest| manifest.compatible(snapshot))
            .map(|manifest| manifest.id.as_str())
            .collect::<Vec<_>>();
        if compatible.is_empty() {
            return SelectionResult::NoScene("no compatible scene for next".into());
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
    active_id: Option<String>,
    next_generation: u64,
}

impl SelectionController {
    pub fn new(registry: SceneRegistry) -> Self {
        Self {
            registry,
            active_id: None,
            next_generation: 0,
        }
    }

    pub fn request(
        &mut self,
        snapshot: EnvironmentSnapshot,
        seed: u64,
        trigger: SelectionTrigger,
    ) -> Result<SelectionRequest, SelectionResult> {
        if !trigger.is_explicit() {
            return Err(SelectionResult::Ignored);
        }
        let result = match trigger {
            SelectionTrigger::Next => self
                .registry
                .select_next(snapshot, self.active_id.as_deref()),
            trigger => self.registry.select(snapshot, seed, trigger),
        };
        match result {
            SelectionResult::Selected(decision) => {
                self.next_generation = self.next_generation.saturating_add(1);
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
        self.next_generation
    }

    pub fn registry(&self) -> &SceneRegistry {
        &self.registry
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneLoadToken {
    pub generation: u64,
    pub id: String,
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
    times: Vec<TimeOfDay>,
    weather: Vec<Weather>,
    seasons: Vec<Season>,
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
        let first = registry.select(snapshot, 42, SelectionTrigger::Reload);
        let second = registry.select(snapshot, 42, SelectionTrigger::Reload);
        assert_eq!(first, second);
        match first {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "night"),
            other => panic!("unexpected selection: {other:?}"),
        }
        assert_eq!(
            registry.select(snapshot, 42, SelectionTrigger::WeatherUpdated),
            SelectionResult::Ignored
        );
        match registry.select(snapshot, 42, SelectionTrigger::ManualId("day".into())) {
            SelectionResult::Selected(decision) => {
                assert_eq!(decision.id, "day");
                assert_eq!(decision.match_score, None);
            }
            other => panic!("unexpected manual selection: {other:?}"),
        }
        assert!(matches!(
            registry.select(snapshot, 42, SelectionTrigger::ManualId("missing".into())),
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
            .request(initial, 1, SelectionTrigger::Startup)
            .unwrap();
        controller.mark_active(&startup.id).unwrap();
        let changed = EnvironmentSnapshot {
            time: TimeOfDay::Night,
            weather: Weather::Clear,
            season: Season::Winter,
        };
        assert!(matches!(
            controller.request(changed, 2, SelectionTrigger::ClockTick),
            Err(SelectionResult::Ignored)
        ));
        assert_eq!(controller.generation(), 1);
        let reload = controller
            .request(changed, 2, SelectionTrigger::Reload)
            .unwrap();
        assert_eq!(reload.generation, 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_values_need_explicit_unknown_or_wildcard_and_fallback_is_used() {
        let (root, registry) = loaded_registry();
        let unknown = EnvironmentSnapshot::unknown();
        match registry.select(unknown, 1, SelectionTrigger::Startup) {
            SelectionResult::Selected(decision) => assert_eq!(decision.id, "day"),
            other => panic!("unexpected selection: {other:?}"),
        }
        let empty = SceneRegistry::empty();
        assert!(matches!(
            empty.select(unknown, 1, SelectionTrigger::Startup),
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
                controller.request(EnvironmentSnapshot::unknown(), 0, trigger),
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
}
