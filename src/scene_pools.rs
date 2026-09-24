//! Versioned S12 library catalog, pool/rules sidecar, and the shared
//! eligibility filter (CR-0008).
//!
//! This module is the single implementation of the two file formats defined in
//! `docs/scene-pools.md`: the curator library catalog (`library.toml`) and the
//! separately versioned pool/scene-rules sidecar (`scene-pools.toml`). The
//! runtime config validator (`config`), the shared automatic-selection filter
//! (`scene_registry`), and the local curator GUI (`curator`) all consume this
//! model, so pool and rule semantics cannot drift between them.
//!
//! Nothing here contacts the network, infers location/timezone, or touches the
//! CR-0006 adaptive registry contract. Unknown sidecar keys are rejected
//! everywhere and every read is bounded and path-safe.
#![allow(dead_code)]

use crate::environment::{EnvironmentSnapshot, TimeOfDay, Weather};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    fmt,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf},
};

pub const CATALOG_SCHEMA_VERSION: u32 = 1;
pub const POOLS_SCHEMA_VERSION: u32 = 1;
pub const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RULES_BYTES: usize = 1024 * 1024;
pub const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SCENES: usize = 256;
const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;

const REQUIRED_CATALOG_KEYS: &[&str] = &[
    "id",
    "package",
    "source_collection",
    "world",
    "appearance",
    "image",
    "image_sha256",
    "palette",
    "palette_sha256",
    "source_format",
    "source_sha256",
    "source_url",
    "source_page",
    "source_title",
    "license_state",
];
const OPTIONAL_CATALOG_KEYS: &[&str] = &[
    "display_name",
    "animation",
    "animation_sha256",
    "time_of_day",
    "time_of_day_sha256",
    "time_of_day_atlas",
    "time_of_day_atlas_sha256",
    "source_cycles",
    "source_cycles_sha256",
    "cycle_semantics",
    "source_filename",
    "license_id",
];

const TIME_OF_DAY_ORDER: &[TimeOfDay] = &[
    TimeOfDay::Dawn,
    TimeOfDay::Day,
    TimeOfDay::Dusk,
    TimeOfDay::Night,
    TimeOfDay::PolarDay,
    TimeOfDay::PolarNight,
];
const WEATHER_ORDER: &[Weather] = &[
    Weather::Clear,
    Weather::Cloudy,
    Weather::Rain,
    Weather::Snow,
    Weather::Storm,
    Weather::Fog,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolsError(String);

impl fmt::Display for PoolsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for PoolsError {}

impl From<std::io::Error> for PoolsError {
    fn from(io_error: std::io::Error) -> Self {
        PoolsError(format!("local file access failed: {io_error}"))
    }
}

fn error(message: impl Into<String>) -> PoolsError {
    PoolsError(message.into())
}

// ---------------------------------------------------------------------------
// Bounded, path-safe file helpers shared by the catalog and sidecar loaders.

pub(crate) fn read_regular_nofollow(
    path: &Path,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, PoolsError> {
    let metadata = fs::symlink_metadata(path).map_err(|io_error| {
        error(format!(
            "cannot read {label} {}: {io_error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(error(format!(
            "{label} must be a non-symlink regular file: {}",
            path.display()
        )));
    }
    if metadata.len() > limit as u64 {
        return Err(error(format!(
            "{label} exceeds {limit} bytes: {}",
            path.display()
        )));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(error(format!(
            "{label} is not a regular file: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(error(format!(
            "{label} exceeds {limit} bytes: {}",
            path.display()
        )));
    }
    Ok(bytes)
}

pub(crate) fn parse_toml(bytes: &[u8], label: &str) -> Result<toml::Value, PoolsError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|io_error| error(format!("{label} is not UTF-8: {io_error}")))?;
    toml::from_str(text)
        .map_err(|parse_error| error(format!("{label} is malformed: {parse_error}")))
}

pub(crate) fn reject_unknown_keys(
    table: &toml::map::Map<String, toml::Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), PoolsError> {
    if let Some(key) = table.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(error(format!("{context} contains unknown key {key:?}")));
    }
    Ok(())
}

pub(crate) fn required_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    context: &str,
) -> Result<String, PoolsError> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| error(format!("{context}.{key} must be a string")))
}

pub(crate) fn optional_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    context: &str,
) -> Result<Option<String>, PoolsError> {
    table
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| error(format!("{context}.{key} must be a string")))
        })
        .transpose()
}

pub(crate) fn validate_scene_id(id: &str) -> Result<(), PoolsError> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && id
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        return Err(error(format!("unsafe scene ID: {id:?}")));
    }
    Ok(())
}

/// Curator-authored display names are bounded plain text and always emitted as
/// simple TOML literals (no quotes, backslashes, or control characters).
fn validate_display_name(name: &str) -> Result<(), PoolsError> {
    let valid = !name.is_empty()
        && name.chars().count() <= 200
        && !name.contains(['"', '\\', '\n', '\r', '\t'])
        && !name.chars().any(|c| c.is_control());
    if !valid {
        return Err(error(
            "display_name must be 1..=200 plain characters without quotes or control characters",
        ));
    }
    Ok(())
}

/// Set or clear one scene's curator-authored display name in `library.toml`
/// with an atomic replace. Provenance fields are never rewritten; only the
/// `display_name` line of the target `[[scene]]` block is added, replaced, or
/// removed, and the complete document is re-parsed before it replaces the
/// previous file (the constraint set keeps every value a simple TOML literal).
pub fn set_catalog_display_name(
    library_root: &Path,
    scene_id: &str,
    display_name: Option<&str>,
) -> Result<(), PoolsError> {
    validate_scene_id(scene_id)?;
    if let Some(name) = display_name {
        validate_display_name(name)?;
    }
    let catalog_path = library_root.join("library.toml");
    let bytes = read_regular_nofollow(&catalog_path, MAX_CATALOG_BYTES, "library catalog")?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|utf8_error| error(format!("library catalog is not UTF-8: {utf8_error}")))?;

    let mut out = String::with_capacity(text.len() + 64);
    let mut current_id: Option<String> = None;
    let mut updated = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "[[scene]]" {
            current_id = None;
        } else if let Some(value) = trimmed.strip_prefix("id = ") {
            current_id = Some(value.trim_matches('"').to_owned());
        }
        let in_target = current_id.as_deref() == Some(scene_id);
        if in_target && trimmed.starts_with("display_name = ") {
            updated = true;
            if let Some(name) = display_name {
                out.push_str(&format!("display_name = \"{name}\"\n"));
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
        if in_target && !updated && trimmed.starts_with("appearance = ") {
            if let Some(name) = display_name {
                out.push_str(&format!("display_name = \"{name}\"\n"));
            }
            updated = true;
        }
    }
    if !updated {
        return Err(error(format!(
            "scene {scene_id:?} is not in the library catalog"
        )));
    }

    // Re-parse the complete document before any write replaces the previous
    // valid file; a failure leaves it untouched.
    let document = parse_toml(out.as_bytes(), "library.toml")?;
    let table = document
        .as_table()
        .ok_or_else(|| error("library.toml must be a TOML table"))?;
    if !table.contains_key("scene") {
        return Err(error("library.toml must contain [[scene]] entries"));
    }

    let staging = library_root.join(format!(".library.toml.tmp-{}", std::process::id()));
    let _ = fs::remove_file(&staging);
    let write = (|| -> Result<(), PoolsError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&staging)?;
        file.write_all(out.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&staging, &catalog_path)?;
        Ok(())
    })();
    if write.is_err() {
        let _ = fs::remove_file(&staging);
    }
    write
}

fn validate_digest(digest: &str, label: &str) -> Result<(), PoolsError> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(error(format!(
            "{label} must be 64 lowercase hexadecimal characters"
        )));
    }
    Ok(())
}

fn validate_simple_name(name: &str, label: &str) -> Result<(), PoolsError> {
    if name.is_empty()
        || name.len() > 255
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || name == "."
        || name == ".."
    {
        return Err(error(format!("unsafe {label}: {name:?}")));
    }
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(error(format!("unsafe {label}: {name:?}")));
    }
    Ok(())
}

fn validate_directory_nofollow(path: &Path, label: &str) -> Result<(), PoolsError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(error(format!(
            "{label} must be a non-symlink directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_regular_asset(package: &Path, name: &str) -> Result<(), PoolsError> {
    let path = package.join(name);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|io_error| error(format!("missing asset {}: {io_error}", path.display())))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(error(format!(
            "asset must be a non-symlink regular file: {}",
            path.display()
        )));
    }
    if metadata.len() > MAX_ASSET_BYTES as u64 {
        return Err(error(format!(
            "asset exceeds {MAX_ASSET_BYTES} bytes: {}",
            path.display()
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Library catalog (`library.toml`, schema v1).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogScene {
    pub id: String,
    pub title: String,
    /// Curator-authored display name; provenance `title` is never rewritten.
    pub display_name: Option<String>,
    pub source_collection: String,
    pub world: String,
    pub appearance: String,
    pub license_state: String,
    pub cycle_semantics: Option<String>,
    pub package: PathBuf,
    pub image: String,
    pub palette: String,
    pub animation: Option<String>,
    pub time_of_day: Option<String>,
    pub time_of_day_atlas: Option<String>,
    pub source_cycles: Option<String>,
}

#[derive(Debug)]
pub struct LibraryCatalog {
    root: PathBuf,
    scenes: Vec<CatalogScene>,
    by_id: HashMap<String, usize>,
}

impl LibraryCatalog {
    /// Load `library.toml` and structurally validate every declared package
    /// path inside `root`. The catalog is the authoritative package list; no
    /// directory discovery happens at consumption time.
    pub fn load(library: &Path) -> Result<Self, PoolsError> {
        let root = fs::canonicalize(library).map_err(|io_error| {
            error(format!(
                "cannot open scene library {}: {io_error}",
                library.display()
            ))
        })?;
        if !fs::metadata(&root)
            .map_err(|io_error| error(format!("cannot inspect scene library: {io_error}")))?
            .is_dir()
        {
            return Err(error(format!(
                "scene library is not a directory: {}",
                root.display()
            )));
        }
        let catalog_path = root.join("library.toml");
        let input = read_regular_nofollow(&catalog_path, MAX_CATALOG_BYTES, "library catalog")?;
        let value = parse_toml(&input, "library.toml")?;
        let table = value
            .as_table()
            .ok_or_else(|| error("library.toml must be a TOML table"))?;
        reject_unknown_keys(table, &["schema_version", "scene"], "library.toml")?;
        if table
            .get("schema_version")
            .and_then(toml::Value::as_integer)
            .is_none_or(|version| version != i64::from(CATALOG_SCHEMA_VERSION))
        {
            return Err(error("library.toml schema_version must be 1"));
        }
        let rows = table
            .get("scene")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| error("library.toml must contain [[scene]] entries"))?;
        if rows.len() > MAX_SCENES {
            return Err(error(format!(
                "library catalog exceeds {MAX_SCENES} scenes"
            )));
        }

        let mut scenes = Vec::with_capacity(rows.len());
        let mut by_id = HashMap::with_capacity(rows.len());
        for (row_index, row) in rows.iter().enumerate() {
            let context = format!("library.toml scene[{}]", row_index + 1);
            let row = row
                .as_table()
                .ok_or_else(|| error(format!("{context} must be a table")))?;
            reject_catalog_keys(row, &context)?;
            for key in REQUIRED_CATALOG_KEYS {
                if !row.contains_key(*key) {
                    return Err(error(format!("{context} is missing {key}")));
                }
            }
            let id = required_string(row, "id", &context)?;
            validate_scene_id(&id)?;
            if by_id.contains_key(&id) {
                return Err(error(format!(
                    "library catalog contains duplicate scene ID {id:?}"
                )));
            }
            let package_name = required_string(row, "package", &context)?;
            validate_simple_name(&package_name, "package path")?;
            let package = root.join(&package_name);
            validate_directory_nofollow(&package, "scene package")?;

            let image = required_string(row, "image", &context)?;
            let palette = required_string(row, "palette", &context)?;
            validate_simple_name(&image, "image asset")?;
            validate_simple_name(&palette, "palette asset")?;
            let mut assets = HashMap::new();
            assets.insert("image", image.clone());
            assets.insert("palette", palette.clone());
            for (asset_key, digest_key) in [
                ("animation", "animation_sha256"),
                ("time_of_day", "time_of_day_sha256"),
                ("time_of_day_atlas", "time_of_day_atlas_sha256"),
                ("source_cycles", "source_cycles_sha256"),
            ] {
                let asset = optional_string(row, asset_key, &context)?;
                let digest = optional_string(row, digest_key, &context)?;
                if asset.is_some() != digest.is_some() {
                    return Err(error(format!(
                        "{context} must declare {asset_key} and {digest_key} together"
                    )));
                }
                if let Some(asset) = asset {
                    validate_simple_name(&asset, "asset path")?;
                    assets.insert(asset_key, asset);
                }
            }
            for asset in assets.values() {
                validate_regular_asset(&package, asset)?;
            }
            for key in ["image_sha256", "palette_sha256", "source_sha256"] {
                validate_digest(&required_string(row, key, &context)?, key)?;
            }
            for digest_key in [
                "animation_sha256",
                "time_of_day_sha256",
                "time_of_day_atlas_sha256",
                "source_cycles_sha256",
            ] {
                if let Some(digest) = optional_string(row, digest_key, &context)? {
                    validate_digest(&digest, digest_key)?;
                }
            }
            if assets.contains_key("animation") && assets.contains_key("source_cycles") {
                return Err(error(format!(
                    "{context} cannot declare both animation and source_cycles"
                )));
            }
            if assets.contains_key("time_of_day") != assets.contains_key("time_of_day_atlas") {
                return Err(error(format!(
                    "{context} must declare time_of_day and time_of_day_atlas together"
                )));
            }
            match (
                assets.contains_key("source_cycles"),
                optional_string(row, "cycle_semantics", &context)?,
            ) {
                (true, Some(value)) if value == "unmapped-source-native" => {}
                (false, None) => {}
                _ => {
                    return Err(error(format!(
                        "{context} has invalid cycle_semantics/source_cycles pairing"
                    )));
                }
            }
            let license_state = required_string(row, "license_state", &context)?;
            if !["unreviewed", "reviewed-granted", "reviewed-withheld"]
                .contains(&license_state.as_str())
            {
                return Err(error(format!("{context} has unsupported license_state")));
            }
            if (license_state == "reviewed-granted") != row.contains_key("license_id") {
                return Err(error(format!(
                    "{context} license_id does not match license_state"
                )));
            }
            for key in [
                "source_collection",
                "world",
                "appearance",
                "source_format",
                "source_url",
                "source_page",
                "source_title",
            ] {
                let _ = required_string(row, key, &context)?;
            }
            validate_scene_manifest(&package, &id, &image, &palette, row)?;
            let display_name = optional_string(row, "display_name", &context)?;
            if let Some(name) = &display_name {
                validate_display_name(name)?;
            }
            let entry = CatalogScene {
                id: id.clone(),
                title: required_string(row, "source_title", &context)?,
                display_name,
                source_collection: required_string(row, "source_collection", &context)?,
                world: required_string(row, "world", &context)?,
                appearance: required_string(row, "appearance", &context)?,
                license_state,
                cycle_semantics: optional_string(row, "cycle_semantics", &context)?,
                package,
                image,
                palette,
                animation: assets.get("animation").cloned(),
                time_of_day: assets.get("time_of_day").cloned(),
                time_of_day_atlas: assets.get("time_of_day_atlas").cloned(),
                source_cycles: assets.get("source_cycles").cloned(),
            };
            by_id.insert(id, scenes.len());
            scenes.push(entry);
        }
        Ok(Self {
            root,
            scenes,
            by_id,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn scenes(&self) -> &[CatalogScene] {
        &self.scenes
    }

    pub fn get(&self, id: &str) -> Option<&CatalogScene> {
        self.by_id.get(id).map(|index| &self.scenes[*index])
    }

    pub fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(id)
    }
}

fn reject_catalog_keys(
    table: &toml::map::Map<String, toml::Value>,
    context: &str,
) -> Result<(), PoolsError> {
    let allowed = REQUIRED_CATALOG_KEYS
        .iter()
        .chain(OPTIONAL_CATALOG_KEYS.iter())
        .copied()
        .collect::<Vec<_>>();
    reject_unknown_keys(table, &allowed, context)
}

fn validate_scene_manifest(
    package: &Path,
    id: &str,
    image: &str,
    palette: &str,
    catalog_row: &toml::map::Map<String, toml::Value>,
) -> Result<(), PoolsError> {
    let manifest_path = package.join("scene.toml");
    let bytes = read_regular_nofollow(&manifest_path, MAX_MANIFEST_BYTES, "scene manifest")?;
    let value = parse_toml(&bytes, "scene.toml")?;
    let table = value
        .as_table()
        .ok_or_else(|| error("scene.toml must be a TOML table"))?;
    let version = table
        .get("schema_version")
        .and_then(toml::Value::as_integer);
    if !matches!(version, Some(1 | 2)) {
        return Err(error("scene.toml schema_version must be 1 or 2"));
    }
    if table.get("id").and_then(toml::Value::as_str) != Some(id)
        || table.get("image").and_then(toml::Value::as_str) != Some(image)
        || table.get("palette").and_then(toml::Value::as_str) != Some(palette)
    {
        return Err(error(format!(
            "scene.toml for {id:?} disagrees with library.toml"
        )));
    }
    for key in ["animation", "time_of_day"] {
        let catalog_asset = catalog_row.get(key).and_then(toml::Value::as_str);
        let manifest_asset = table.get(key).and_then(toml::Value::as_str);
        if catalog_asset != manifest_asset {
            return Err(error(format!(
                "scene.toml {key} for {id:?} disagrees with library.toml"
            )));
        }
        if let Some(asset) = manifest_asset {
            validate_simple_name(asset, "scene manifest asset")?;
            validate_regular_asset(package, asset)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pool/rules sidecar (`scene-pools.toml`, schema v1).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolSource {
    Library,
    Registry,
}

impl PoolSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Registry => "registry",
        }
    }

    fn parse(value: &str) -> Result<Self, PoolsError> {
        match value {
            "library" => Ok(Self::Library),
            "registry" => Ok(Self::Registry),
            other => Err(error(format!("unsupported pool source {other:?}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolEntry {
    pub id: String,
    pub source: PoolSource,
    pub enabled: bool,
    pub scene_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneRule {
    pub enabled: bool,
    pub time_of_day: Option<BTreeSet<TimeOfDay>>,
    pub weather_any: Option<BTreeSet<Weather>>,
    pub temperature_min_c: Option<f32>,
    pub temperature_max_c: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePools {
    pools: Vec<PoolEntry>,
    rules: BTreeMap<String, SceneRule>,
    membership: BTreeMap<String, usize>,
}

impl ScenePools {
    /// Parse and fully validate one sidecar document. Unknown keys, duplicate
    /// pool/scene/rule IDs, cross-pool duplicate membership, unpooled rules,
    /// and malformed criteria are all rejected before any state is built.
    pub fn parse(text: &str) -> Result<Self, PoolsError> {
        let value: toml::Value = toml::from_str(text).map_err(|parse_error| {
            error(format!("scene-pools.toml is malformed: {parse_error}"))
        })?;
        let table = value
            .as_table()
            .ok_or_else(|| error("scene-pools.toml must be a TOML table"))?;
        reject_unknown_keys(
            table,
            &["schema_version", "pool", "scene_rule"],
            "scene-pools.toml",
        )?;
        if table
            .get("schema_version")
            .and_then(toml::Value::as_integer)
            .is_none_or(|version| version != i64::from(POOLS_SCHEMA_VERSION))
        {
            return Err(error("scene-pools.toml schema_version must be 1"));
        }
        let pool_rows = table
            .get("pool")
            .and_then(toml::Value::as_array)
            .ok_or_else(|| error("scene-pools.toml must contain [[pool]] entries"))?;
        let rule_rows = table
            .get("scene_rule")
            .and_then(toml::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if pool_rows.len() > MAX_SCENES || rule_rows.len() > MAX_SCENES {
            return Err(error(format!(
                "scene-pools.toml exceeds {MAX_SCENES} pools or rules"
            )));
        }

        let mut pools = Vec::with_capacity(pool_rows.len());
        for (index, row) in pool_rows.iter().enumerate() {
            let context = format!("scene-pools.toml pool #{index}");
            let row = row
                .as_table()
                .ok_or_else(|| error(format!("{context} must be a table")))?;
            reject_unknown_keys(row, &["id", "source", "enabled", "scene_ids"], &context)?;
            let id = required_string(row, "id", &context)?;
            validate_scene_id(&id)?;
            let source = PoolSource::parse(&required_string(row, "source", &context)?)?;
            let enabled = row
                .get("enabled")
                .and_then(toml::Value::as_bool)
                .ok_or_else(|| error(format!("{context}.enabled must be a boolean")))?;
            let scene_ids = match row.get("scene_ids") {
                Some(toml::Value::Array(values)) => {
                    let mut ids = Vec::with_capacity(values.len());
                    for value in values {
                        let id = value
                            .as_str()
                            .ok_or_else(|| error(format!("{context}.scene_ids must be strings")))?;
                        validate_scene_id(id)?;
                        ids.push(id.to_owned());
                    }
                    ids
                }
                _ => return Err(error(format!("{context}.scene_ids must be an array"))),
            };
            pools.push(PoolEntry {
                id,
                source,
                enabled,
                scene_ids,
            });
        }

        let mut rules = Vec::with_capacity(rule_rows.len());
        for (index, row) in rule_rows.iter().enumerate() {
            let context = format!("scene-pools.toml scene_rule #{index}");
            let row = row
                .as_table()
                .ok_or_else(|| error(format!("{context} must be a table")))?;
            reject_unknown_keys(
                row,
                &[
                    "scene_id",
                    "enabled",
                    "time_of_day",
                    "temperature_min_c",
                    "temperature_max_c",
                    "weather_any",
                ],
                &context,
            )?;
            let scene_id = required_string(row, "scene_id", &context)?;
            validate_scene_id(&scene_id)?;
            let enabled = row
                .get("enabled")
                .and_then(toml::Value::as_bool)
                .ok_or_else(|| error(format!("{context}.enabled must be a boolean")))?;
            let time_of_day = parse_time_values(row, &context)?;
            let weather_any = parse_weather_values(row, &context)?;
            let (temperature_min_c, temperature_max_c) = parse_temperatures(row, &context)?;
            rules.push((
                scene_id,
                SceneRule {
                    enabled,
                    time_of_day,
                    weather_any,
                    temperature_min_c,
                    temperature_max_c,
                },
            ));
        }
        Self::from_parts(pools, rules)
    }

    /// Read and parse one bounded sidecar file. The path must become (or be) a
    /// non-symlink regular file; a missing file is reported as an error and is
    /// only "optional" at the caller's discretion.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, PoolsError> {
        let bytes = read_regular_nofollow(path.as_ref(), MAX_RULES_BYTES, "pool rules")?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|utf8_error| error(format!("pool rules are not UTF-8: {utf8_error}")))?;
        Self::parse(text)
    }

    /// Cross-validate authored pools and rules into one immutable model.
    pub fn from_parts(
        pools: Vec<PoolEntry>,
        rules: impl IntoIterator<Item = (String, SceneRule)>,
    ) -> Result<Self, PoolsError> {
        if pools.is_empty() {
            return Err(error("scene-pools.toml must contain at least one [[pool]]"));
        }
        let mut pool_ids = BTreeSet::new();
        let mut membership = BTreeMap::new();
        for (index, pool) in pools.iter().enumerate() {
            validate_scene_id(&pool.id)
                .map_err(|_| error(format!("unsafe pool ID {:?}", pool.id)))?;
            if !pool_ids.insert(pool.id.clone()) {
                return Err(error(format!("duplicate pool ID {:?}", pool.id)));
            }
            for scene_id in &pool.scene_ids {
                validate_scene_id(scene_id)?;
                if let Some(existing) = membership.insert(scene_id.clone(), index) {
                    let _ = existing;
                    return Err(error(format!(
                        "scene {scene_id:?} is already claimed by pool {:?}",
                        pools[index].id
                    )));
                }
            }
        }
        let mut rules_map = BTreeMap::new();
        for (scene_id, rule) in rules {
            validate_scene_id(&scene_id)?;
            if !membership.contains_key(&scene_id) {
                return Err(error(format!(
                    "scene_rule for {scene_id:?} does not belong to any pool"
                )));
            }
            if rules_map.insert(scene_id.clone(), rule).is_some() {
                return Err(error(format!("duplicate rule for scene {scene_id:?}")));
            }
        }
        Ok(Self {
            pools,
            rules: rules_map,
            membership,
        })
    }

    /// Deterministic TOML document used for atomic sidecar saves. Pool order
    /// and pool field order are preserved; rules are emitted in scene-ID order
    /// with canonical criterion ordering.
    pub fn to_toml_string(&self) -> String {
        let mut out = String::new();
        out.push_str("schema_version = 1\n");
        for pool in &self.pools {
            out.push_str("\n[[pool]]\n");
            out.push_str(&format!("id = \"{}\"\n", pool.id));
            out.push_str(&format!("source = \"{}\"\n", pool.source.as_str()));
            out.push_str(&format!("enabled = {}\n", pool.enabled));
            let ids = pool
                .scene_ids
                .iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("scene_ids = [{ids}]\n"));
        }
        for (scene_id, rule) in &self.rules {
            out.push_str("\n[[scene_rule]]\n");
            out.push_str(&format!("scene_id = \"{scene_id}\"\n"));
            out.push_str(&format!("enabled = {}\n", rule.enabled));
            if let Some(times) = &rule.time_of_day {
                let values = TIME_OF_DAY_ORDER
                    .iter()
                    .filter(|time| times.contains(time))
                    .map(|time| format!("\"{}\"", time_of_day_token(*time)))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("time_of_day = [{values}]\n"));
            }
            if let Some(minimum) = rule.temperature_min_c {
                out.push_str(&format!("temperature_min_c = {}\n", format_float(minimum)));
            }
            if let Some(maximum) = rule.temperature_max_c {
                out.push_str(&format!("temperature_max_c = {}\n", format_float(maximum)));
            }
            if let Some(weathers) = &rule.weather_any {
                let values = WEATHER_ORDER
                    .iter()
                    .filter(|weather| weathers.contains(weather))
                    .map(|weather| format!("\"{}\"", weather_token(*weather)))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("weather_any = [{values}]\n"));
            }
        }
        out
    }

    pub fn pools(&self) -> &[PoolEntry] {
        &self.pools
    }

    pub fn rules(&self) -> &BTreeMap<String, SceneRule> {
        &self.rules
    }

    pub fn rule(&self, scene_id: &str) -> Option<&SceneRule> {
        self.rules.get(scene_id)
    }

    pub fn membership(&self, scene_id: &str) -> Option<&PoolEntry> {
        self.membership
            .get(scene_id)
            .map(|index| &self.pools[*index])
    }

    pub fn contains_scene(&self, scene_id: &str) -> bool {
        self.membership.contains_key(scene_id)
    }

    /// Scene IDs from `source` pools in stable pool-then-membership order.
    pub fn pool_scene_ids(&self, source: PoolSource) -> impl Iterator<Item = &str> {
        self.pools
            .iter()
            .filter(move |pool| pool.source == source)
            .flat_map(|pool| pool.scene_ids.iter().map(String::as_str))
    }

    /// The one shared eligibility evaluation for every automatic selection
    /// path. A scene outside every pool is never an automatic candidate; a
    /// configured criterion with an unknown required input does not match.
    pub fn evaluate(&self, scene_id: &str, environment: &PoolEnvironment) -> Eligibility {
        let mut reasons = Vec::new();
        let Some(pool) = self.membership(scene_id) else {
            reasons.push(EligibilityReason::NotInPool);
            return Eligibility { reasons };
        };
        if !pool.enabled {
            reasons.push(EligibilityReason::PoolDisabled);
        }
        if let Some(rule) = self.rules.get(scene_id) {
            if !rule.enabled {
                reasons.push(EligibilityReason::SceneDisabled);
            }
            if let Some(times) = &rule.time_of_day {
                match environment.time {
                    TimeOfDay::Unknown => reasons.push(EligibilityReason::TimeOfDayUnknown),
                    time if !times.contains(&time) => {
                        reasons.push(EligibilityReason::TimeOfDayMismatch);
                    }
                    _ => {}
                }
            }
            if let Some(weathers) = &rule.weather_any {
                match environment.weather {
                    Weather::Unknown => reasons.push(EligibilityReason::WeatherUnknown),
                    weather if !weathers.contains(&weather) => {
                        reasons.push(EligibilityReason::WeatherMismatch);
                    }
                    _ => {}
                }
            }
            if rule.temperature_min_c.is_some() || rule.temperature_max_c.is_some() {
                match environment.known_temperature_c() {
                    None => reasons.push(EligibilityReason::TemperatureUnknown),
                    Some(temperature) => {
                        let below = rule
                            .temperature_min_c
                            .is_some_and(|minimum| temperature < minimum);
                        let above = rule
                            .temperature_max_c
                            .is_some_and(|maximum| temperature > maximum);
                        if below || above {
                            reasons.push(EligibilityReason::TemperatureOutOfRange);
                        }
                    }
                }
            }
        }
        Eligibility { reasons }
    }

    /// Preserve the caller's candidate order while permitting only eligible
    /// scenes. This is the filter every automatic selection path shares.
    pub fn eligible_ids<'a>(
        &self,
        candidates: impl IntoIterator<Item = &'a str>,
        environment: &PoolEnvironment,
    ) -> Vec<String> {
        candidates
            .into_iter()
            .filter(|id| self.evaluate(id, environment).eligible())
            .map(str::to_owned)
            .collect()
    }
}

fn parse_time_values(
    row: &toml::map::Map<String, toml::Value>,
    context: &str,
) -> Result<Option<BTreeSet<TimeOfDay>>, PoolsError> {
    let Some(value) = row.get("time_of_day") else {
        return Ok(None);
    };
    let mut times = BTreeSet::new();
    for value in tag_values(value, "time_of_day", context)? {
        let time = time_of_day_from_token(value, context)?;
        if !times.insert(time) {
            return Err(error(format!(
                "{context}.time_of_day contains a duplicate value"
            )));
        }
    }
    Ok(Some(times))
}

fn parse_weather_values(
    row: &toml::map::Map<String, toml::Value>,
    context: &str,
) -> Result<Option<BTreeSet<Weather>>, PoolsError> {
    let Some(value) = row.get("weather_any") else {
        return Ok(None);
    };
    let mut weathers = BTreeSet::new();
    for value in tag_values(value, "weather_any", context)? {
        let weather = weather_from_token(value, context)?;
        if !weathers.insert(weather) {
            return Err(error(format!(
                "{context}.weather_any contains a duplicate value"
            )));
        }
    }
    Ok(Some(weathers))
}

fn tag_values<'a>(
    value: &'a toml::Value,
    key: &str,
    context: &str,
) -> Result<Vec<&'a str>, PoolsError> {
    let values = value
        .as_array()
        .filter(|values| !values.is_empty())
        .ok_or_else(|| error(format!("{context}.{key} must be a non-empty string array")))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| error(format!("{context}.{key} values must be strings")))
        })
        .collect()
}

fn parse_temperatures(
    row: &toml::map::Map<String, toml::Value>,
    context: &str,
) -> Result<(Option<f32>, Option<f32>), PoolsError> {
    let mut minimum = None;
    let mut maximum = None;
    for (key, target) in [
        ("temperature_min_c", &mut minimum),
        ("temperature_max_c", &mut maximum),
    ] {
        let Some(value) = row.get(key) else { continue };
        let number = value
            .as_float()
            .or_else(|| value.as_integer().map(|value| value as f64))
            .filter(|value| value.is_finite() && (-100.0..=100.0).contains(value))
            .ok_or_else(|| {
                error(format!(
                    "{context}.{key} must be a finite number in -100..=100"
                ))
            })?;
        *target = Some(number as f32);
    }
    if matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum > maximum) {
        return Err(error(format!(
            "{context} temperature_min_c must not exceed temperature_max_c"
        )));
    }
    Ok((minimum, maximum))
}

fn time_of_day_token(time: TimeOfDay) -> &'static str {
    match time {
        TimeOfDay::Dawn => "dawn",
        TimeOfDay::Day => "day",
        TimeOfDay::Dusk => "dusk",
        TimeOfDay::Night => "night",
        TimeOfDay::PolarDay => "polar_day",
        TimeOfDay::PolarNight => "polar_night",
        TimeOfDay::Unknown => "unknown",
    }
}

fn time_of_day_from_token(value: &str, context: &str) -> Result<TimeOfDay, PoolsError> {
    Ok(match value {
        "dawn" => TimeOfDay::Dawn,
        "day" => TimeOfDay::Day,
        "dusk" => TimeOfDay::Dusk,
        "night" => TimeOfDay::Night,
        "polar_day" => TimeOfDay::PolarDay,
        "polar_night" => TimeOfDay::PolarNight,
        _ => {
            return Err(error(format!(
                "{context}.time_of_day contains an unsupported value"
            )));
        }
    })
}

fn weather_token(weather: Weather) -> &'static str {
    match weather {
        Weather::Clear => "clear",
        Weather::Cloudy => "cloudy",
        Weather::Rain => "rain",
        Weather::Snow => "snow",
        Weather::Storm => "storm",
        Weather::Fog => "fog",
        Weather::Unknown => "unknown",
    }
}

fn weather_from_token(value: &str, context: &str) -> Result<Weather, PoolsError> {
    Ok(match value {
        "clear" => Weather::Clear,
        "cloudy" => Weather::Cloudy,
        "rain" => Weather::Rain,
        "snow" => Weather::Snow,
        "storm" => Weather::Storm,
        "fog" => Weather::Fog,
        _ => {
            return Err(error(format!(
                "{context}.weather_any contains an unsupported value"
            )));
        }
    })
}

fn format_float(value: f32) -> String {
    // TOML distinguishes floats from integers by a fractional part; keep every
    // authored number a float on save so round-trips preserve the value type.
    let text = format!("{value:?}");
    if text.contains(['.', 'e', 'E']) {
        text
    } else {
        format!("{text}.0")
    }
}

/// Parse helper for the curator's JSON edit payload and preview queries.
pub fn time_of_day_from_name(value: &str) -> Option<TimeOfDay> {
    time_of_day_from_token(value, "").ok()
}

pub fn weather_from_name(value: &str) -> Option<Weather> {
    weather_from_token(value, "").ok()
}

pub fn time_of_day_name(time: TimeOfDay) -> &'static str {
    time_of_day_token(time)
}

pub fn weather_name(weather: Weather) -> &'static str {
    weather_token(weather)
}

// ---------------------------------------------------------------------------
// Eligibility.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibilityReason {
    NotInPool,
    PoolDisabled,
    SceneDisabled,
    TimeOfDayUnknown,
    TimeOfDayMismatch,
    WeatherUnknown,
    WeatherMismatch,
    TemperatureUnknown,
    TemperatureOutOfRange,
}

impl EligibilityReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotInPool => "not-in-pool",
            Self::PoolDisabled => "pool-disabled",
            Self::SceneDisabled => "scene-disabled",
            Self::TimeOfDayUnknown => "time-of-day-unknown",
            Self::TimeOfDayMismatch => "time-of-day-mismatch",
            Self::WeatherUnknown => "weather-unknown",
            Self::WeatherMismatch => "weather-mismatch",
            Self::TemperatureUnknown => "temperature-unknown",
            Self::TemperatureOutOfRange => "temperature-out-of-range",
        }
    }

    /// Bounded human-readable explanation. Reasons describe rule categories
    /// only and never include coordinates or provider request details.
    pub const fn as_detail(self) -> &'static str {
        match self {
            Self::NotInPool => "the scene does not belong to any pool",
            Self::PoolDisabled => "its pool is disabled",
            Self::SceneDisabled => "the scene is disabled",
            Self::TimeOfDayUnknown => "a time-of-day criterion needs a known time",
            Self::TimeOfDayMismatch => "the preview time is outside the time-of-day criterion",
            Self::WeatherUnknown => "a weather criterion needs known weather",
            Self::WeatherMismatch => "the preview weather is outside the weather_any criterion",
            Self::TemperatureUnknown => "a temperature criterion needs a known temperature",
            Self::TemperatureOutOfRange => "the temperature is outside the configured range",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eligibility {
    pub reasons: Vec<EligibilityReason>,
}

impl Eligibility {
    pub fn eligible(&self) -> bool {
        self.reasons.is_empty()
    }
}

/// One immutable environment input per selection attempt. Temperature is the
/// latest known value or `None` when it is unknown/unavailable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoolEnvironment {
    pub time: TimeOfDay,
    pub weather: Weather,
    pub temperature_c: Option<f32>,
}

impl PoolEnvironment {
    pub fn new(snapshot: &EnvironmentSnapshot, temperature_c: Option<f32>) -> Self {
        Self {
            time: snapshot.time,
            weather: snapshot.weather,
            temperature_c: temperature_c.filter(|value| value.is_finite()),
        }
    }

    /// Only a finite value inside the authored bounds counts as known input.
    pub fn known_temperature_c(self) -> Option<f32> {
        self.temperature_c
            .filter(|value| (-100.0..=100.0).contains(value))
    }
}

/// The shared gate passed to every automatic selection path. `None` means no
/// sidecar is configured and preserves legacy selection behavior exactly.
#[derive(Debug, Clone, Copy)]
pub struct PoolGate<'a> {
    pub pools: &'a ScenePools,
    pub environment: &'a PoolEnvironment,
}

impl PoolGate<'_> {
    pub fn is_eligible(&self, scene_id: &str) -> bool {
        self.pools.evaluate(scene_id, self.environment).eligible()
    }

    pub fn evaluate(&self, scene_id: &str) -> Eligibility {
        self.pools.evaluate(scene_id, self.environment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    const SAMPLE: &str = r#"schema_version = 1

[[pool]]
id = "canvascycle"
source = "library"
enabled = true
scene_ids = ["canvascycle-v25heat", "canvascycle-v02"]

[[pool]]
id = "registry"
source = "registry"
enabled = false
scene_ids = ["v29-seascape"]

[[scene_rule]]
scene_id = "canvascycle-v25heat"
enabled = true
time_of_day = ["day", "dawn"]
temperature_min_c = 28.0
temperature_max_c = 45.0
weather_any = ["clear", "cloudy"]

[[scene_rule]]
scene_id = "v29-seascape"
enabled = false
"#;

    fn environment(
        time: TimeOfDay,
        weather: Weather,
        temperature_c: Option<f32>,
    ) -> PoolEnvironment {
        PoolEnvironment {
            time,
            weather,
            temperature_c,
        }
    }

    #[test]
    fn display_name_bound_matches_catalog_character_count() {
        assert!(validate_display_name(&"é".repeat(200)).is_ok());
        assert!(validate_display_name(&"é".repeat(201)).is_err());
    }

    #[test]
    fn sidecar_parses_and_round_trips_deterministically() {
        let pools = ScenePools::parse(SAMPLE).unwrap();
        assert_eq!(pools.pools().len(), 2);
        assert_eq!(
            pools.pools()[0].source,
            PoolSource::Library,
            "library source is preserved"
        );
        let rendered = pools.to_toml_string();
        let reparsed = ScenePools::parse(&rendered).unwrap();
        assert_eq!(pools, reparsed, "save/load round-trip preserves the model");
        // Canonical ordering: dawn before day, clear before cloudy.
        assert!(
            rendered.contains("time_of_day = [\"dawn\", \"day\"]"),
            "canonical time ordering: {rendered}"
        );
        assert!(
            rendered.contains("weather_any = [\"clear\", \"cloudy\"]"),
            "canonical weather ordering: {rendered}"
        );
        assert!(
            rendered.contains("temperature_min_c = 28.0"),
            "floats stay floats: {rendered}"
        );
    }

    #[test]
    fn eligibility_covers_unknowns_boundaries_and_pool_scene_state() {
        let pools = ScenePools::parse(SAMPLE).unwrap();
        let matches = environment(TimeOfDay::Day, Weather::Clear, Some(28.0));
        assert!(pools.evaluate("canvascycle-v25heat", &matches).eligible());
        let exact_top = environment(TimeOfDay::Dawn, Weather::Cloudy, Some(45.0));
        assert!(pools.evaluate("canvascycle-v25heat", &exact_top).eligible());
        assert_eq!(
            pools
                .evaluate(
                    "canvascycle-v25heat",
                    &environment(TimeOfDay::Night, Weather::Clear, Some(30.0))
                )
                .reasons,
            vec![EligibilityReason::TimeOfDayMismatch]
        );
        assert_eq!(
            pools
                .evaluate(
                    "canvascycle-v25heat",
                    &environment(TimeOfDay::Day, Weather::Rain, Some(30.0))
                )
                .reasons,
            vec![EligibilityReason::WeatherMismatch]
        );
        assert_eq!(
            pools
                .evaluate(
                    "canvascycle-v25heat",
                    &environment(TimeOfDay::Day, Weather::Clear, Some(27.9))
                )
                .reasons,
            vec![EligibilityReason::TemperatureOutOfRange]
        );
        // Unknown required inputs never match a configured criterion.
        assert_eq!(
            pools
                .evaluate(
                    "canvascycle-v25heat",
                    &environment(TimeOfDay::Unknown, Weather::Clear, Some(30.0))
                )
                .reasons,
            vec![EligibilityReason::TimeOfDayUnknown]
        );
        assert_eq!(
            pools
                .evaluate(
                    "canvascycle-v25heat",
                    &environment(TimeOfDay::Day, Weather::Unknown, Some(30.0))
                )
                .reasons,
            vec![EligibilityReason::WeatherUnknown]
        );
        assert_eq!(
            pools
                .evaluate(
                    "canvascycle-v25heat",
                    &environment(TimeOfDay::Day, Weather::Clear, None)
                )
                .reasons,
            vec![EligibilityReason::TemperatureUnknown]
        );
        // A disabled pool or scene is not eligible even with matching criteria.
        let disabled = environment(TimeOfDay::Day, Weather::Clear, Some(30.0));
        assert_eq!(
            pools.evaluate("v29-seascape", &disabled).reasons,
            vec![
                EligibilityReason::PoolDisabled,
                EligibilityReason::SceneDisabled
            ]
        );
        // Scenes outside every pool are never automatic candidates.
        assert_eq!(
            pools.evaluate("unpooled", &disabled).reasons,
            vec![EligibilityReason::NotInPool]
        );
    }

    #[test]
    fn eligibility_without_rules_only_needs_pool_and_scene_state() {
        let pools = ScenePools::parse(SAMPLE).unwrap();
        // canvascycle-v02 has no rule: only pool state gates it.
        let unknown = environment(TimeOfDay::Unknown, Weather::Unknown, None);
        assert!(pools.evaluate("canvascycle-v02", &unknown).eligible());
    }

    #[test]
    fn malformed_sidecars_fail_without_partial_state() {
        for document in [
            "schema_version = 2\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = []\n",
            "schema_version = 1\n",
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = []\nextra = 1\n",
            // Duplicate membership across pools, including across sources.
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"s1\"]\n[[pool]]\nid = \"b\"\nsource = \"registry\"\nenabled = true\nscene_ids = [\"s1\"]\n",
            // Duplicate pool IDs.
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = []\n[[pool]]\nid = \"a\"\nsource = \"registry\"\nenabled = true\nscene_ids = []\n",
            // Unpooled rule.
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = []\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\n",
            // Duplicate rule.
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"s1\"]\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\n",
            // Empty and unknown criterion values.
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"s1\"]\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\ntime_of_day = []\n",
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"s1\"]\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\nweather_any = [\"hail\"]\n",
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"s1\"]\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\ntime_of_day = [\"day\", \"day\"]\n",
            // Inverted or out-of-range temperatures.
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"s1\"]\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\ntemperature_min_c = 30.0\ntemperature_max_c = 20.0\n",
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"s1\"]\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\ntemperature_min_c = 101.0\n",
            // Unsafe IDs.
            "schema_version = 1\n[[pool]]\nid = \"../a\"\nsource = \"library\"\nenabled = true\nscene_ids = []\n",
            // Unknown rule key.
            "schema_version = 1\n[[pool]]\nid = \"a\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"s1\"]\n[[scene_rule]]\nscene_id = \"s1\"\nenabled = true\nseasons = [\"winter\"]\n",
        ] {
            assert!(
                ScenePools::parse(document).is_err(),
                "document must be rejected: {document}"
            );
        }
    }

    #[test]
    fn load_rejects_symlinks_and_oversized_files() {
        let root = std::env::temp_dir().join(format!(
            "worldwall-scene-pools-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("scene-pools.toml");
        fs::write(&path, SAMPLE).unwrap();
        assert!(ScenePools::load(&path).is_ok());

        let link = root.join("link.toml");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(
            ScenePools::load(&link).is_err(),
            "symlink input is rejected"
        );

        let missing = root.join("missing.toml");
        assert!(
            ScenePools::load(&missing).is_err(),
            "missing input is an error"
        );

        fs::write(&path, vec![b' '; MAX_RULES_BYTES + 1]).unwrap();
        assert!(
            ScenePools::load(&path).is_err(),
            "oversized input is rejected"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn eligible_ids_preserve_candidate_order() {
        let pools = ScenePools::parse(SAMPLE).unwrap();
        let env = environment(TimeOfDay::Day, Weather::Clear, Some(30.0));
        assert_eq!(
            pools.eligible_ids(
                ["canvascycle-v25heat", "v29-seascape", "canvascycle-v02"],
                &env
            ),
            ["canvascycle-v25heat", "canvascycle-v02"]
        );
    }
}
