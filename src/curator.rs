//! Read-only local scene catalog browser for the S12 AC3 implementation slice.
//!
//! The curator serves only validated catalog metadata and decoded indexed
//! scene/palette bytes on an ephemeral loopback listener. It launches an
//! isolated Firefox profile and never connects to Worldwall IPC.

use crate::scene;
use std::{
    collections::{HashMap, HashSet},
    env,
    error::Error,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::{Child, Command, ExitStatus},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;
const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
const MAX_SCENES: usize = 256;
const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CuratorOptions {
    pub library: PathBuf,
    pub rules: PathBuf,
    pub rules_optional: bool,
}

#[derive(Debug, Clone)]
struct SceneEntry {
    id: String,
    title: String,
    source_collection: String,
    world: String,
    appearance: String,
    license_state: String,
    cycle_semantics: Option<String>,
    package: PathBuf,
    image: String,
    palette: String,
    animation: bool,
}

#[derive(Debug)]
struct Catalog {
    scenes: Vec<SceneEntry>,
    by_id: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
struct PoolSummary {
    id: String,
    source: String,
    enabled: bool,
    scene_count: usize,
}

#[derive(Debug, Clone)]
struct RulesSummary {
    path: PathBuf,
    pools: Vec<PoolSummary>,
    rule_count: usize,
}

#[derive(Debug)]
struct AppState {
    catalog: Catalog,
    rules: Option<RulesSummary>,
    token: String,
}

struct TempProfile(PathBuf);

impl Drop for TempProfile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct HttpResponse {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("usage: worldwallctl curator [--library DIR] [--rules FILE]");
        return Ok(());
    }
    let options = parse_options(args)?;
    let catalog = load_catalog(&options.library)?;
    let rules = load_rules(&options.rules, options.rules_optional)?;
    let token = random_token()?;
    let state = Arc::new(AppState {
        catalog,
        rules,
        token: token.clone(),
    });

    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server_state = Arc::clone(&state);
    let server = thread::Builder::new()
        .name("worldwall-curator-http".into())
        .spawn(move || serve(listener, server_state, server_stop))?;

    let profile = match TempProfile::create() {
        Ok(profile) => profile,
        Err(error) => {
            stop_server(&stop, address, server);
            return Err(error.into());
        }
    };
    let url = format!("http://{address}/{token}/");
    eprintln!(
        "worldwallctl curator: opening a local read-only catalog in isolated Firefox; Worldwall is not contacted"
    );
    let child_result = Command::new("firefox")
        .env("MOZ_ENABLE_WAYLAND", "1")
        .arg("--no-remote")
        .arg("--profile")
        .arg(&profile.0)
        .arg("--new-window")
        .arg(url)
        .spawn();

    let status = match child_result {
        Ok(mut child) => wait_for_browser(&mut child),
        Err(error) => Err(error),
    };
    stop_server(&stop, address, server);
    drop(profile);
    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("Firefox exited with status {status}").into()),
        Err(error) => Err(format!("could not open isolated Firefox curator: {error}").into()),
    }
}

fn wait_for_browser(child: &mut Child) -> io::Result<ExitStatus> {
    child.wait()
}

fn stop_server(stop: &AtomicBool, address: SocketAddr, server: thread::JoinHandle<()>) {
    stop.store(true, Ordering::Release);
    let _ = TcpStream::connect_timeout(&address, Duration::from_millis(100));
    let _ = server.join();
}

fn parse_options(args: &[String]) -> Result<CuratorOptions, Box<dyn Error>> {
    let mut library = None;
    let mut rules = None;
    let mut index = 0;
    while index < args.len() {
        let option = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{option} requires a path"))?;
        if value.starts_with("--") {
            return Err(format!("{option} requires a path").into());
        }
        match option {
            "--library" if library.is_none() => library = Some(PathBuf::from(value)),
            "--rules" if rules.is_none() => rules = Some(PathBuf::from(value)),
            "--library" | "--rules" => {
                return Err(format!("{option} may be specified only once").into());
            }
            _ => return Err(format!("unknown curator option: {option}").into()),
        }
        index += 2;
    }

    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;
    let data_home = xdg_base("XDG_DATA_HOME", &home, ".local/share");
    let config_home = xdg_base("XDG_CONFIG_HOME", &home, ".config");
    let rules_optional = rules.is_none();
    Ok(CuratorOptions {
        library: library.unwrap_or_else(|| data_home.join("worldwall/scene-library")),
        rules: rules.unwrap_or_else(|| config_home.join("worldwall/scene-pools.toml")),
        rules_optional,
    })
}

fn xdg_base(variable: &str, home: &Path, fallback: &str) -> PathBuf {
    xdg_base_value(env::var_os(variable).as_deref(), home, fallback)
}

fn xdg_base_value(value: Option<&OsStr>, home: &Path, fallback: &str) -> PathBuf {
    value
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(fallback))
}

fn load_catalog(library: &Path) -> Result<Catalog, Box<dyn Error>> {
    let root = fs::canonicalize(library)
        .map_err(|error| format!("cannot open scene library {}: {error}", library.display()))?;
    if !fs::metadata(&root)?.is_dir() {
        return Err(format!("scene library is not a directory: {}", root.display()).into());
    }
    let catalog_path = root.join("library.toml");
    let input = read_regular_nofollow(&catalog_path, MAX_CATALOG_BYTES, "library catalog")?;
    let value = parse_toml(&input, "library.toml")?;
    let table = value
        .as_table()
        .ok_or("library.toml must be a TOML table")?;
    reject_unknown_keys(table, &["schema_version", "scene"], "library.toml")?;
    if table
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(1)
    {
        return Err("library.toml schema_version must be 1".into());
    }
    let rows = table
        .get("scene")
        .and_then(toml::Value::as_array)
        .ok_or("library.toml must contain [[scene]] entries")?;
    if rows.len() > MAX_SCENES {
        return Err(format!("library catalog exceeds {MAX_SCENES} scenes").into());
    }

    let mut scenes = Vec::with_capacity(rows.len());
    let mut by_id = HashMap::with_capacity(rows.len());
    for (row_index, row) in rows.iter().enumerate() {
        let where_ = format!("library.toml scene[{}]", row_index + 1);
        let row = row
            .as_table()
            .ok_or_else(|| format!("{where_} must be a table"))?;
        reject_catalog_keys(row, &where_)?;
        for key in REQUIRED_CATALOG_KEYS {
            if !row.contains_key(*key) {
                return Err(format!("{where_} is missing {key}").into());
            }
        }
        let id = required_string(row, "id", &where_)?;
        validate_scene_id(&id)?;
        if by_id.contains_key(&id) {
            return Err(format!("library catalog contains duplicate scene ID {id:?}").into());
        }
        let package_name = required_string(row, "package", &where_)?;
        validate_simple_name(&package_name, "package path")?;
        let package = root.join(&package_name);
        validate_directory_nofollow(&package, "scene package")?;

        let image = required_string(row, "image", &where_)?;
        let palette = required_string(row, "palette", &where_)?;
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
            let asset = optional_string(row, asset_key, &where_)?;
            let digest = optional_string(row, digest_key, &where_)?;
            if asset.is_some() != digest.is_some() {
                return Err(
                    format!("{where_} must declare {asset_key} and {digest_key} together").into(),
                );
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
            validate_digest(&required_string(row, key, &where_)?, key)?;
        }
        for digest_key in [
            "animation_sha256",
            "time_of_day_sha256",
            "time_of_day_atlas_sha256",
            "source_cycles_sha256",
        ] {
            if let Some(digest) = optional_string(row, digest_key, &where_)? {
                validate_digest(&digest, digest_key)?;
            }
        }
        if assets.contains_key("animation") && assets.contains_key("source_cycles") {
            return Err(format!("{where_} cannot declare both animation and source_cycles").into());
        }
        if assets.contains_key("time_of_day") != assets.contains_key("time_of_day_atlas") {
            return Err(format!(
                "{where_} must declare time_of_day and time_of_day_atlas together"
            )
            .into());
        }
        match (
            assets.contains_key("source_cycles"),
            optional_string(row, "cycle_semantics", &where_)?,
        ) {
            (true, Some(value)) if value == "unmapped-source-native" => {}
            (false, None) => {}
            _ => {
                return Err(
                    format!("{where_} has invalid cycle_semantics/source_cycles pairing").into(),
                );
            }
        }
        let license_state = required_string(row, "license_state", &where_)?;
        if !["unreviewed", "reviewed-granted", "reviewed-withheld"]
            .contains(&license_state.as_str())
        {
            return Err(format!("{where_} has unsupported license_state").into());
        }
        if (license_state == "reviewed-granted") != row.contains_key("license_id") {
            return Err(format!("{where_} license_id does not match license_state").into());
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
            let _ = required_string(row, key, &where_)?;
        }
        validate_scene_manifest(&package, &id, &image, &palette, row)?;
        let entry = SceneEntry {
            id: id.clone(),
            title: required_string(row, "source_title", &where_)?,
            source_collection: required_string(row, "source_collection", &where_)?,
            world: required_string(row, "world", &where_)?,
            appearance: required_string(row, "appearance", &where_)?,
            license_state,
            cycle_semantics: optional_string(row, "cycle_semantics", &where_)?,
            package,
            image,
            palette,
            animation: assets.contains_key("animation"),
        };
        by_id.insert(id, scenes.len());
        scenes.push(entry);
    }
    Ok(Catalog { scenes, by_id })
}

fn reject_catalog_keys(
    table: &toml::map::Map<String, toml::Value>,
    where_: &str,
) -> Result<(), Box<dyn Error>> {
    let allowed = REQUIRED_CATALOG_KEYS
        .iter()
        .chain(OPTIONAL_CATALOG_KEYS.iter())
        .copied()
        .collect::<Vec<_>>();
    reject_unknown_keys(table, &allowed, where_)
}

fn reject_unknown_keys(
    table: &toml::map::Map<String, toml::Value>,
    allowed: &[&str],
    where_: &str,
) -> Result<(), Box<dyn Error>> {
    if let Some(key) = table.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{where_} contains unknown key {key:?}").into());
    }
    Ok(())
}

fn required_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    where_: &str,
) -> Result<String, Box<dyn Error>> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{where_}.{key} must be a string").into())
}

fn optional_string(
    table: &toml::map::Map<String, toml::Value>,
    key: &str,
    where_: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    table
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{where_}.{key} must be a string").into())
        })
        .transpose()
}

fn validate_scene_manifest(
    package: &Path,
    id: &str,
    image: &str,
    palette: &str,
    catalog_row: &toml::map::Map<String, toml::Value>,
) -> Result<(), Box<dyn Error>> {
    let manifest_path = package.join("scene.toml");
    let bytes = read_regular_nofollow(&manifest_path, MAX_MANIFEST_BYTES, "scene manifest")?;
    let value = parse_toml(&bytes, "scene.toml")?;
    let table = value.as_table().ok_or("scene.toml must be a TOML table")?;
    let version = table
        .get("schema_version")
        .and_then(toml::Value::as_integer);
    if !matches!(version, Some(1 | 2)) {
        return Err("scene.toml schema_version must be 1 or 2".into());
    }
    if table.get("id").and_then(toml::Value::as_str) != Some(id)
        || table.get("image").and_then(toml::Value::as_str) != Some(image)
        || table.get("palette").and_then(toml::Value::as_str) != Some(palette)
    {
        return Err(format!("scene.toml for {id:?} disagrees with library.toml").into());
    }
    for key in ["animation", "time_of_day"] {
        let catalog_asset = catalog_row.get(key).and_then(toml::Value::as_str);
        let manifest_asset = table.get(key).and_then(toml::Value::as_str);
        if catalog_asset != manifest_asset {
            return Err(format!("scene.toml {key} for {id:?} disagrees with library.toml").into());
        }
        if let Some(asset) = manifest_asset {
            validate_simple_name(asset, "scene manifest asset")?;
            validate_regular_asset(package, asset)?;
        }
    }
    Ok(())
}

fn validate_scene_id(id: &str) -> Result<(), Box<dyn Error>> {
    let valid = id.len() <= 64
        && id
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        return Err(format!("unsafe scene ID: {id:?}").into());
    }
    Ok(())
}

fn validate_digest(digest: &str, label: &str) -> Result<(), Box<dyn Error>> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("{label} must be 64 lowercase hexadecimal characters").into());
    }
    Ok(())
}

fn validate_simple_name(name: &str, label: &str) -> Result<(), Box<dyn Error>> {
    if name.is_empty()
        || name.len() > 255
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || name == "."
        || name == ".."
    {
        return Err(format!("unsafe {label}: {name:?}").into());
    }
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!("unsafe {label}: {name:?}").into());
    }
    Ok(())
}

fn validate_directory_nofollow(path: &Path, label: &str) -> Result<(), Box<dyn Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{label} must be a non-symlink directory: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn validate_regular_asset(package: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    let path = package.join(name);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("missing asset {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "asset must be a non-symlink regular file: {}",
            path.display()
        )
        .into());
    }
    if metadata.len() > MAX_ASSET_BYTES as u64 {
        return Err(format!("asset exceeds {MAX_ASSET_BYTES} bytes: {}", path.display()).into());
    }
    Ok(())
}

fn parse_toml(bytes: &[u8], label: &str) -> Result<toml::Value, Box<dyn Error>> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("{label} is not UTF-8: {error}"))?;
    Ok(toml::from_str(text)?)
}

fn read_regular_nofollow(
    path: &Path,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot read {label} {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{label} must be a non-symlink regular file: {}",
            path.display()
        )
        .into());
    }
    if metadata.len() > limit as u64 {
        return Err(format!("{label} exceeds {limit} bytes: {}", path.display()).into());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(format!("{label} is not a regular file: {}", path.display()).into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(format!("{label} exceeds {limit} bytes: {}", path.display()).into());
    }
    Ok(bytes)
}

fn load_rules(path: &Path, optional: bool) -> Result<Option<RulesSummary>, Box<dyn Error>> {
    let bytes = match read_regular_nofollow(path, MAX_CATALOG_BYTES, "pool rules") {
        Ok(bytes) => bytes,
        Err(_error)
            if optional
                && fs::symlink_metadata(path)
                    .is_err_and(|error| error.kind() == io::ErrorKind::NotFound) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let value = parse_toml(&bytes, "scene-pools.toml")?;
    let table = value
        .as_table()
        .ok_or("scene-pools.toml must be a TOML table")?;
    reject_unknown_keys(
        table,
        &["schema_version", "pool", "scene_rule"],
        "scene-pools.toml",
    )?;
    if table
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        != Some(1)
    {
        return Err("scene-pools.toml schema_version must be 1".into());
    }
    let pools = table
        .get("pool")
        .and_then(toml::Value::as_array)
        .ok_or("scene-pools.toml must contain [[pool]] entries")?;
    let rules = table
        .get("scene_rule")
        .and_then(toml::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if pools.is_empty() {
        return Err("scene-pools.toml must contain at least one [[pool]]".into());
    }
    if pools.len() > MAX_SCENES {
        return Err(format!("sidecar exceeds {MAX_SCENES} pools").into());
    }
    let mut summaries = Vec::with_capacity(pools.len());
    let mut pool_ids = HashSet::new();
    let mut members = HashSet::new();
    for pool in pools {
        let pool = pool.as_table().ok_or("each pool must be a TOML table")?;
        reject_unknown_keys(
            pool,
            &["id", "source", "enabled", "scene_ids"],
            "scene-pools.toml pool",
        )?;
        let id = pool
            .get("id")
            .and_then(toml::Value::as_str)
            .ok_or("pool id must be a string")?;
        validate_scene_id(id)?;
        if !pool_ids.insert(id.to_owned()) {
            return Err(format!("duplicate pool ID {id:?}").into());
        }
        let source = pool
            .get("source")
            .and_then(toml::Value::as_str)
            .ok_or("pool source must be a string")?;
        if !matches!(source, "library" | "registry") {
            return Err(format!("unsupported pool source {source:?}").into());
        }
        let enabled = pool
            .get("enabled")
            .and_then(toml::Value::as_bool)
            .ok_or("pool enabled must be a boolean")?;
        let scene_ids = pool
            .get("scene_ids")
            .and_then(toml::Value::as_array)
            .ok_or("pool scene_ids must be an array")?;
        for scene_id in scene_ids {
            let scene_id = scene_id
                .as_str()
                .ok_or("pool scene_ids must contain only strings")?;
            validate_scene_id(scene_id)?;
            if !members.insert(scene_id.to_owned()) {
                return Err(format!("scene {scene_id:?} occurs in more than one pool").into());
            }
        }
        summaries.push(PoolSummary {
            id: id.to_owned(),
            source: source.to_owned(),
            enabled,
            scene_count: scene_ids.len(),
        });
    }
    let mut rule_ids = HashSet::new();
    for rule in rules {
        let rule = rule
            .as_table()
            .ok_or("each scene_rule must be a TOML table")?;
        let allowed = [
            "scene_id",
            "enabled",
            "time_of_day",
            "temperature_min_c",
            "temperature_max_c",
            "weather_any",
        ];
        reject_unknown_keys(rule, &allowed, "scene-pools.toml scene_rule")?;
        let scene_id = rule
            .get("scene_id")
            .and_then(toml::Value::as_str)
            .ok_or("scene_rule scene_id must be a string")?;
        validate_scene_id(scene_id)?;
        if !members.contains(scene_id) || !rule_ids.insert(scene_id.to_owned()) {
            return Err(format!("scene_rule for {scene_id:?} is duplicate or unpooled").into());
        }
        if rule.get("enabled").and_then(toml::Value::as_bool).is_none() {
            return Err(format!("scene_rule enabled must be a boolean for {scene_id:?}").into());
        }
        validate_rule_values(rule, scene_id)?;
    }
    Ok(Some(RulesSummary {
        path: path.to_path_buf(),
        pools: summaries,
        rule_count: rules.len(),
    }))
}

fn validate_rule_values(
    rule: &toml::map::Map<String, toml::Value>,
    scene_id: &str,
) -> Result<(), Box<dyn Error>> {
    for (key, allowed) in [
        (
            "time_of_day",
            &["dawn", "day", "dusk", "night", "polar_day", "polar_night"][..],
        ),
        (
            "weather_any",
            &["clear", "cloudy", "rain", "snow", "storm", "fog"][..],
        ),
    ] {
        let Some(value) = rule.get(key) else { continue };
        let values = value
            .as_array()
            .filter(|values| !values.is_empty())
            .ok_or_else(|| {
                format!("scene_rule {key} must be a non-empty string array for {scene_id:?}")
            })?;
        let mut seen = HashSet::new();
        for value in values {
            let value = value.as_str().ok_or_else(|| {
                format!("scene_rule {key} values must be strings for {scene_id:?}")
            })?;
            if !allowed.contains(&value) || !seen.insert(value) {
                return Err(format!(
                    "scene_rule {key} contains an unsupported or duplicate value for {scene_id:?}"
                )
                .into());
            }
        }
    }
    let mut minimum = None;
    let mut maximum = None;
    for (key, target) in [
        ("temperature_min_c", &mut minimum),
        ("temperature_max_c", &mut maximum),
    ] {
        let Some(value) = rule.get(key) else { continue };
        let number = value
            .as_float()
            .or_else(|| value.as_integer().map(|value| value as f64))
            .filter(|value| value.is_finite() && (-100.0..=100.0).contains(value))
            .ok_or_else(|| {
                format!("scene_rule {key} must be a finite number in -100..=100 for {scene_id:?}")
            })?;
        *target = Some(number);
    }
    if matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum > maximum) {
        return Err(
            format!("scene_rule temperature minimum exceeds maximum for {scene_id:?}").into(),
        );
    }
    Ok(())
}

fn random_token() -> io::Result<String> {
    let mut bytes = [0u8; 24];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

impl TempProfile {
    fn write_private_prefs(&self) -> io::Result<()> {
        const PREFS: &str = r#"user_pref("toolkit.telemetry.enabled", false);
user_pref("toolkit.telemetry.unified", false);
user_pref("datareporting.healthreport.uploadEnabled", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("app.normandy.enabled", false);
user_pref("app.shield.optoutstudies.enabled", false);
user_pref("app.update.enabled", false);
user_pref("browser.ping-centre.telemetry", false);
user_pref("browser.newtabpage.activity-stream.feeds.telemetry", false);
user_pref("browser.newtabpage.activity-stream.telemetry", false);
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("network.captive-portal-service.enabled", false);
user_pref("network.connectivity-service.enabled", false);
"#;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.0.join("user.js"))?;
        file.write_all(PREFS.as_bytes())?;
        file.sync_all()
    }

    fn create() -> io::Result<Self> {
        let parent = env::temp_dir();
        for attempt in 0..8u32 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = parent.join(format!(
                "worldwall-curator-{}-{nanos}-{attempt}",
                std::process::id()
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    if let Err(error) =
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                    {
                        let _ = fs::remove_dir_all(&path);
                        return Err(error);
                    }
                    let profile = Self(path);
                    if let Err(error) = profile.write_private_prefs() {
                        let _ = fs::remove_dir_all(&profile.0);
                        return Err(error);
                    }
                    return Ok(profile);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate private Firefox profile directory",
        ))
    }
}

fn serve(listener: TcpListener, state: Arc<AppState>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _peer)) => {
                let _ = handle_client(stream, &state);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
}

fn handle_client(mut stream: TcpStream, state: &AppState) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = match read_request_target(&stream) {
        Ok(target) => target,
        Err(_) => {
            write_response(&mut stream, not_found())?;
            return Ok(());
        }
    };
    let response = route(state, &request);
    write_response(&mut stream, response)
}

fn read_request_target(stream: &TcpStream) -> io::Result<String> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut total = 0usize;
    let mut first_line = String::new();
    total += (&mut reader)
        .take(MAX_HTTP_HEADER_BYTES as u64)
        .read_line(&mut first_line)?;
    if total == 0 || total >= MAX_HTTP_HEADER_BYTES || !first_line.ends_with("\r\n") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid HTTP request line",
        ));
    }
    let mut fields = first_line.trim_end_matches("\r\n").split(' ');
    if fields.next() != Some("GET") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "only GET is supported",
        ));
    }
    let target = fields
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing HTTP target"))?;
    if fields.next() != Some("HTTP/1.1") || fields.next().is_some() || !target.starts_with('/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid HTTP request line",
        ));
    }
    loop {
        let mut line = String::new();
        let read = (&mut reader)
            .take((MAX_HTTP_HEADER_BYTES - total) as u64)
            .read_line(&mut line)?;
        total += read;
        if read == 0 || total >= MAX_HTTP_HEADER_BYTES || !line.ends_with("\r\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid or oversized HTTP headers",
            ));
        }
        if line == "\r\n" {
            break;
        }
    }
    Ok(target.to_owned())
}

fn route(state: &AppState, path: &str) -> HttpResponse {
    let prefix = format!("/{}/", state.token);
    let Some(relative) = path.strip_prefix(&prefix) else {
        return not_found();
    };
    if relative.is_empty() {
        return response(
            "200 OK",
            "text/html; charset=utf-8",
            include_str!("bin/curator.html").as_bytes().to_vec(),
        );
    }
    if relative == "app.js" {
        return response(
            "200 OK",
            "text/javascript; charset=utf-8",
            include_str!("bin/curator.js").as_bytes().to_vec(),
        );
    }
    if relative == "catalog" {
        return match serde_json::to_vec(&catalog_json(state)) {
            Ok(body) => response("200 OK", "application/json; charset=utf-8", body),
            Err(_) => response(
                "500 Internal Server Error",
                "text/plain; charset=utf-8",
                b"catalog serialization failed".to_vec(),
            ),
        };
    }
    if let Some(rest) = relative.strip_prefix("preview/") {
        let mut parts = rest.split('/');
        let Some(id) = parts.next() else {
            return not_found();
        };
        let Some(kind) = parts.next() else {
            return not_found();
        };
        if parts.next().is_some() {
            return not_found();
        }
        return match preview_asset(state, id, kind) {
            Ok((content_type, body)) => response("200 OK", content_type, body),
            Err(error) => response(
                "422 Unprocessable Content",
                "text/plain; charset=utf-8",
                error.to_string().into_bytes(),
            ),
        };
    }
    not_found()
}

fn catalog_json(state: &AppState) -> Value {
    let scenes = state
        .catalog
        .scenes
        .iter()
        .map(|scene| {
            json!({
                "id": scene.id,
                "title": scene.title,
                "source_collection": scene.source_collection,
                "world": scene.world,
                "appearance": scene.appearance,
                "license_state": scene.license_state,
                "cycle_semantics": scene.cycle_semantics,
                "animation_manifest_present": scene.animation,
            })
        })
        .collect::<Vec<_>>();
    let rules = state.rules.as_ref().map(|summary| {
        json!({
            "path": summary.path.display().to_string(),
            "pools": summary.pools.iter().map(|pool| json!({
                "id": pool.id,
                "source": pool.source,
                "enabled": pool.enabled,
                "scene_count": pool.scene_count,
            })).collect::<Vec<_>>(),
            "rule_count": summary.rule_count,
        })
    });
    json!({
        "scenes": scenes,
        "rules": rules,
        "message": "Read-only AC3 implementation slice: editing and eligibility evaluation are not implemented.",
    })
}

fn preview_asset(
    state: &AppState,
    id: &str,
    kind: &str,
) -> Result<(&'static str, Vec<u8>), Box<dyn Error>> {
    let index = *state.catalog.by_id.get(id).ok_or("unknown scene ID")?;
    let scene = &state.catalog.scenes[index];
    match kind {
        "indices" => {
            let bytes = read_regular_nofollow(
                &scene.package.join(&scene.image),
                MAX_ASSET_BYTES,
                "scene PNG",
            )?;
            let decoded = scene::decode_index_png(&bytes)?;
            let mut body = Vec::with_capacity(8 + decoded.indices.len());
            body.extend_from_slice(&decoded.width.to_be_bytes());
            body.extend_from_slice(&decoded.height.to_be_bytes());
            body.extend_from_slice(&decoded.indices);
            Ok(("application/octet-stream", body))
        }
        "palette" => {
            let bytes = read_regular_nofollow(
                &scene.package.join(&scene.palette),
                MAX_ASSET_BYTES,
                "palette PNG",
            )?;
            let decoded = scene::decode_palette_png(&bytes)?;
            Ok(("application/octet-stream", decoded.rgba.to_vec()))
        }
        _ => Err("unknown preview asset kind".into()),
    }
}

fn response(status: &'static str, content_type: &'static str, body: Vec<u8>) -> HttpResponse {
    HttpResponse {
        status,
        content_type,
        body,
    }
}

fn not_found() -> HttpResponse {
    response(
        "404 Not Found",
        "text/plain; charset=utf-8",
        b"not found".to_vec(),
    )
}

fn write_response(stream: &mut TcpStream, response: HttpResponse) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::{BitDepth, ColorType};
    use std::{
        os::unix::fs::symlink,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn temporary_root(label: &str) -> PathBuf {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "worldwall-curator-test-{}-{label}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn png_bytes(width: u32, height: u32, color: ColorType, bytes: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, width, height);
            encoder.set_color(color);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(bytes).unwrap();
        }
        output
    }

    fn sample_library(root: &Path) {
        let package = root.join("sample-package");
        fs::create_dir_all(&package).unwrap();
        let indices = png_bytes(2, 1, ColorType::Grayscale, &[0, 255]);
        let mut palette = vec![0u8; 256 * 4];
        for entry in palette.chunks_exact_mut(4) {
            entry.copy_from_slice(&[10, 20, 30, 255]);
        }
        fs::write(package.join("scene.png"), &indices).unwrap();
        fs::write(
            package.join("palette.png"),
            png_bytes(256, 1, ColorType::Rgba, &palette),
        )
        .unwrap();
        fs::write(package.join("scene.toml"), "schema_version = 1\nid = \"sample\"\nimage = \"scene.png\"\npalette = \"palette.png\"\n").unwrap();
        fs::write(
            root.join("library.toml"),
            r#"schema_version = 1
[[scene]]
id = "sample"
package = "sample-package"
source_collection = "local"
world = "unknown"
appearance = "unknown"
image = "scene.png"
image_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
palette = "palette.png"
palette_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
source_format = "local-test"
source_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"
source_url = "https://example.invalid/source"
source_page = "https://example.invalid/"
source_title = "Sample scene"
license_state = "unreviewed"
"#,
        )
        .unwrap();
    }

    #[test]
    fn curator_options_use_xdg_defaults_and_allow_explicit_overrides() {
        let home = env::var_os("HOME").unwrap_or_else(|| OsStr::new("/tmp").to_os_string());
        let options = parse_options(&[]).unwrap();
        let home_path = PathBuf::from(home);
        let data_home = xdg_base("XDG_DATA_HOME", &home_path, ".local/share");
        let config_home = xdg_base("XDG_CONFIG_HOME", &home_path, ".config");
        assert_eq!(options.library, data_home.join("worldwall/scene-library"));
        assert_eq!(
            options.rules,
            config_home.join("worldwall/scene-pools.toml")
        );
        assert!(options.rules_optional);
        assert_eq!(
            xdg_base_value(
                Some(OsStr::new("relative/path")),
                Path::new("/home/test"),
                ".config"
            ),
            PathBuf::from("/home/test/.config")
        );
        assert_eq!(
            xdg_base_value(
                Some(OsStr::new("/absolute/xdg")),
                Path::new("/home/test"),
                ".config"
            ),
            PathBuf::from("/absolute/xdg")
        );
        let explicit = parse_options(&[
            "--library".into(),
            "/tmp/library".into(),
            "--rules".into(),
            "/tmp/rules.toml".into(),
        ])
        .unwrap();
        assert_eq!(explicit.library, PathBuf::from("/tmp/library"));
        assert_eq!(explicit.rules, PathBuf::from("/tmp/rules.toml"));
        assert!(!explicit.rules_optional);
    }

    #[test]
    fn curator_options_reject_duplicates_unknowns_and_missing_values() {
        assert!(parse_options(&["--other".into(), "x".into()]).is_err());
        assert!(parse_options(&["--library".into()]).is_err());
        assert!(
            parse_options(&[
                "--library".into(),
                "a".into(),
                "--library".into(),
                "b".into()
            ])
            .is_err()
        );
    }

    #[test]
    fn catalog_loads_safe_metadata_and_preview_decodes_index_palette() {
        let root = temporary_root("catalog");
        sample_library(&root);
        let catalog = load_catalog(&root).unwrap();
        assert_eq!(catalog.scenes.len(), 1);
        assert_eq!(catalog.scenes[0].title, "Sample scene");
        let state = AppState {
            catalog,
            rules: None,
            token: "token".into(),
        };
        let (_, indices) = preview_asset(&state, "sample", "indices").unwrap();
        assert_eq!(&indices[..8], &[0, 0, 0, 2, 0, 0, 0, 1]);
        assert_eq!(&indices[8..], &[0, 255]);
        let (_, palette) = preview_asset(&state, "sample", "palette").unwrap();
        assert_eq!(palette.len(), 1024);
        assert_eq!(&palette[..4], &[10, 20, 30, 255]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn catalog_rejects_escaping_or_symlinked_package_assets_and_duplicate_ids() {
        let root = temporary_root("unsafe");
        sample_library(&root);
        let package = root.join("sample-package");
        let outside = root.join("outside.png");
        fs::copy(package.join("scene.png"), &outside).unwrap();
        fs::remove_file(package.join("scene.png")).unwrap();
        symlink(&outside, package.join("scene.png")).unwrap();
        assert!(load_catalog(&root).is_err());
        fs::remove_file(package.join("scene.png")).unwrap();
        fs::copy(&outside, package.join("scene.png")).unwrap();
        let mut catalog = fs::read_to_string(root.join("library.toml")).unwrap();
        catalog.push_str("\n[[scene]]\nid = \"sample\"\n");
        fs::write(root.join("library.toml"), catalog).unwrap();
        assert!(load_catalog(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_routes_are_token_scoped_and_do_not_accept_filesystem_paths() {
        let root = temporary_root("routes");
        sample_library(&root);
        let state = AppState {
            catalog: load_catalog(&root).unwrap(),
            rules: None,
            token: "random-token".into(),
        };
        assert_eq!(route(&state, "/wrong/catalog").status, "404 Not Found");
        assert_eq!(
            route(&state, "/random-token/preview/../outside/indices").status,
            "404 Not Found"
        );
        assert_eq!(
            route(&state, "/random-token/preview/sample/indices").status,
            "200 OK"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loopback_http_serves_token_scoped_catalog_json() {
        let root = temporary_root("http");
        sample_library(&root);
        let state = AppState {
            catalog: load_catalog(&root).unwrap(),
            rules: None,
            token: "local-token".into(),
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_client(stream, &state).unwrap();
        });
        let mut client = TcpStream::connect(address).unwrap();
        client
            .write_all(b"GET /local-token/catalog HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        server.join().unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("Sample scene"));
        assert!(!response.contains("scene.png"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sidecar_summary_is_read_only_and_checks_pool_rule_structure() {
        let root = temporary_root("rules");
        let rules = root.join("scene-pools.toml");
        fs::write(
            &rules,
            "schema_version = 1\n[[pool]]\nid = \"local-scenes\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"sample\"]\n[[scene_rule]]\nscene_id = \"sample\"\nenabled = true\ntime_of_day = [\"day\"]\ntemperature_min_c = 20.0\nweather_any = [\"clear\", \"cloudy\"]\n",
        )
        .unwrap();
        let summary = load_rules(&rules, false).unwrap().unwrap();
        assert_eq!(summary.pools.len(), 1);
        assert_eq!(summary.pools[0].id, "local-scenes");
        assert_eq!(summary.rule_count, 1);
        assert!(load_rules(&root.join("missing.toml"), false).is_err());
        assert!(
            load_rules(&root.join("missing.toml"), true)
                .unwrap()
                .is_none()
        );
        fs::write(
            &rules,
            "schema_version = 1\n[[pool]]\nid = \"local-scenes\"\nsource = \"library\"\nenabled = true\nscene_ids = [\"sample\"]\n[[scene_rule]]\nscene_id = \"sample\"\nenabled = true\ntemperature_min_c = 35.0\ntemperature_max_c = 20.0\n",
        )
        .unwrap();
        assert!(load_rules(&rules, false).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn private_firefox_profile_directory_is_mode_0700_and_removed() {
        let profile = TempProfile::create().unwrap();
        assert_eq!(
            fs::metadata(&profile.0).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let prefs = profile.0.join("user.js");
        assert_eq!(
            fs::metadata(&prefs).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(
            fs::read_to_string(&prefs)
                .unwrap()
                .contains("toolkit.telemetry.enabled",)
        );
        let path = profile.0.clone();
        drop(profile);
        assert!(!path.exists());
    }

    #[test]
    fn preview_rejects_non_indexed_and_transparent_png_inputs() {
        let root = temporary_root("png-reject");
        sample_library(&root);
        let package = root.join("sample-package");
        let rgba = png_bytes(2, 1, ColorType::Rgba, &[1, 2, 3, 255, 1, 2, 3, 255]);
        fs::write(package.join("scene.png"), rgba).unwrap();
        let state = AppState {
            catalog: load_catalog(&root).unwrap(),
            rules: None,
            token: "t".into(),
        };
        assert!(preview_asset(&state, "sample", "indices").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
