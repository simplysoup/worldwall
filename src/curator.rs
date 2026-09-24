//! Local scene-pool curator for the S12 AC3/AC6/AC7 implementation slices.
//!
//! The curator serves validated catalog metadata, decoded indexed scene and
//! palette bytes, authored animation metadata, and time-of-day atlases on an
//! ephemeral token-scoped loopback listener. It edits only its own pool/rules
//! sidecar (atomically, same-user permissions) and can request an explicit
//! `reload-config` from a running Worldwall. It never starts, stops, or
//! terminates the wallpaper process and never contacts the network.

use crate::{
    animation::Animation,
    environment::{TimeOfDay, Weather},
    ipc::{self, Command},
    scene,
    scene_pools::{
        self, EligibilityReason, LibraryCatalog, PoolEntry, PoolEnvironment, PoolSource,
        ScenePools, SceneRule,
    },
    scene_registry,
};
use std::{
    collections::{BTreeSet, HashMap},
    env,
    error::Error,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_REQUEST_BODY_BYTES: usize = 256 * 1024;
const MAX_PREVIEW_BYTES: usize = scene_pools::MAX_ASSET_BYTES;
const MAX_PREVIEW_PIXELS: u64 = 16_777_216;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CuratorOptions {
    pub library: PathBuf,
    pub rules: PathBuf,
    pub rules_optional: bool,
}

#[derive(Debug)]
struct EditorState {
    pools: Option<ScenePools>,
    rules_path: PathBuf,
    last_save: Option<String>,
}

/// One browsable scene package from either configured source: the library
/// catalog or the configured Worldwall registry (pool-sourced scenes).
#[derive(Debug, Clone)]
struct PreviewScene {
    id: String,
    title: String,
    display_name: Option<String>,
    source: &'static str,
    source_collection: String,
    world: String,
    appearance: String,
    license_state: String,
    cycle_semantics: Option<String>,
    source_cycles: bool,
    image: PathBuf,
    palette: PathBuf,
    animation: Option<PathBuf>,
    time_of_day: Option<PathBuf>,
}

impl PreviewScene {
    fn from_catalog(entry: &scene_pools::CatalogScene) -> Self {
        Self {
            id: entry.id.clone(),
            title: entry.title.clone(),
            display_name: entry.display_name.clone(),
            source: "library",
            source_collection: entry.source_collection.clone(),
            world: entry.world.clone(),
            appearance: entry.appearance.clone(),
            license_state: entry.license_state.clone(),
            cycle_semantics: entry.cycle_semantics.clone(),
            source_cycles: entry.source_cycles.is_some(),
            image: entry.package.join(&entry.image),
            palette: entry.package.join(&entry.palette),
            animation: entry
                .animation
                .as_ref()
                .map(|name| entry.package.join(name)),
            time_of_day: entry
                .time_of_day
                .as_ref()
                .map(|name| entry.package.join(name)),
        }
    }

    fn from_manifest(manifest: &scene_registry::SceneManifest) -> Self {
        Self {
            id: manifest.id.clone(),
            title: manifest.id.clone(),
            display_name: None,
            source: "registry",
            source_collection: "registry".to_owned(),
            world: manifest
                .family_id
                .clone()
                .unwrap_or_else(|| "unknown".to_owned()),
            appearance: manifest.weather.first().map_or_else(
                || "unknown".to_owned(),
                |weather| scene_pools::weather_name(*weather).to_owned(),
            ),
            // Registry packages carry no catalog license record; do not
            // invent one.
            license_state: "not-recorded".to_owned(),
            cycle_semantics: None,
            source_cycles: false,
            image: manifest.image_path.clone(),
            palette: manifest.palette_path.clone(),
            animation: manifest.animation_path.clone(),
            time_of_day: manifest.time_of_day_path.clone(),
        }
    }
}

#[derive(Debug)]
struct AppState {
    scenes: Vec<PreviewScene>,
    by_id: HashMap<String, usize>,
    library_root: PathBuf,
    display_names: Mutex<HashMap<String, String>>,
    registry_note: String,
    editor: Mutex<EditorState>,
    token: String,
}

impl AppState {
    fn scene(&self, id: &str) -> Option<&PreviewScene> {
        self.by_id.get(id).map(|index| &self.scenes[*index])
    }

    fn display_name(&self, scene: &PreviewScene) -> Option<String> {
        self.display_names
            .lock()
            .expect("display-name lock")
            .get(&scene.id)
            .cloned()
            .or_else(|| scene.display_name.clone())
    }
}

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("usage: worldwallctl curator [--library DIR] [--rules FILE]");
        return Ok(());
    }
    let options = parse_options(args)?;
    let catalog = LibraryCatalog::load(&options.library)?;
    let mut scenes = catalog
        .scenes()
        .iter()
        .map(PreviewScene::from_catalog)
        .collect::<Vec<_>>();
    let registry_note = match load_registry_scenes() {
        Ok(registry_scenes) => {
            let mut added = 0;
            for scene in registry_scenes {
                // On a duplicate ID across sources the library catalog wins.
                if !scenes.iter().any(|known| known.id == scene.id) {
                    scenes.push(scene);
                    added += 1;
                }
            }
            format!("{added} registry scene(s) loaded")
        }
        Err(error) => {
            format!("registry scenes unavailable ({error}); browsing the library only")
        }
    };
    let library_root = catalog.root().to_path_buf();
    let mut display_names = HashMap::new();
    for scene in &scenes {
        if let Some(name) = &scene.display_name {
            display_names.insert(scene.id.clone(), name.clone());
        }
    }
    let mut by_id = HashMap::new();
    for (index, scene) in scenes.iter().enumerate() {
        by_id.entry(scene.id.clone()).or_insert(index);
    }
    let pools = load_rules(&options.rules, options.rules_optional)?;
    let token = random_token()?;
    let state = Arc::new(AppState {
        scenes,
        by_id,
        library_root,
        display_names: Mutex::new(display_names),
        registry_note,
        editor: Mutex::new(EditorState {
            pools,
            rules_path: options.rules.clone(),
            last_save: None,
        }),
        token: token.clone(),
    });

    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let server_state = Arc::clone(&state);
    let _server = thread::Builder::new()
        .name("worldwall-curator-http".into())
        .spawn(move || serve(listener, server_state, server_stop))?;

    let url = format!("http://{address}/{token}/");
    println!(
        "worldwallctl curator: open {url} in your browser (local, token-scoped; Ctrl+C to stop)"
    );
    // Serve until interrupted. The curator never contacts Worldwall except by
    // an explicit reload request from the page.
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
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

fn load_rules(path: &Path, optional: bool) -> Result<Option<ScenePools>, Box<dyn Error>> {
    match ScenePools::load(path) {
        Ok(pools) => Ok(Some(pools)),
        Err(_error)
            if optional
                && fs::symlink_metadata(path)
                    .is_err_and(|error| error.kind() == io::ErrorKind::NotFound) =>
        {
            // A missing default rules file means no sidecar is present yet.
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

/// Atomically replace the sidecar with same-user-only permissions. The
/// rendered document must validate before any write happens; a failure at any
/// step leaves the previous valid file untouched.
fn save_rules(path: &Path, document: &str) -> Result<(), Box<dyn Error>> {
    let validated = ScenePools::parse(document)?;
    if validated.to_toml_string() != document {
        return Err("sidecar document is not in canonical save form".into());
    }
    if let Some(parent) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|error| format!("cannot create rules directory: {error}"))?;
    }
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.file_type().is_symlink()
    {
        return Err("refusing to replace a symlinked sidecar".into());
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("tmp-{}-{unique}", std::process::id()));
    let write_result = (|| -> Result<(), Box<dyn Error>> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)?;
        file.write_all(document.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        if let Some(parent) = path.parent()
            && let Ok(directory) = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_CLOEXEC)
                .open(parent)
        {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result?;
    if let Ok(metadata) = fs::symlink_metadata(path) {
        let mode = metadata.permissions().mode() & 0o777;
        if mode != 0o600 {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP server (loopback, random token, GET/POST only).

struct HttpRequest {
    method: String,
    target: String,
    body: Vec<u8>,
}

struct HttpResponse {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
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
    let request = match read_request(&stream) {
        Ok(request) => request,
        Err(_) => {
            write_response(&mut stream, not_found())?;
            return Ok(());
        }
    };
    let response = route(state, &request);
    write_response(&mut stream, response)
}

fn read_request(stream: &TcpStream) -> io::Result<HttpRequest> {
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
    let method = fields.next().unwrap_or_default();
    if method != "GET" && method != "POST" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "only GET and POST are supported",
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
    let mut content_length = 0usize;
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
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid content length")
            })?;
        }
    }
    if content_length > MAX_REQUEST_BODY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized HTTP body",
        ));
    }
    let mut body = vec![0; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(HttpRequest {
        method: method.to_owned(),
        target: target.to_owned(),
        body,
    })
}

fn route(state: &AppState, request: &HttpRequest) -> HttpResponse {
    let prefix = format!("/{}/", state.token);
    let Some(relative) = request.target.strip_prefix(&prefix) else {
        return not_found();
    };
    let path = relative.split('?').next().unwrap_or_default();
    let query = relative.split_once('?').map(|(_, query)| query);
    match (request.method.as_str(), path) {
        ("GET", "") => response(
            "200 OK",
            "text/html; charset=utf-8",
            include_str!("bin/curator.html").as_bytes().to_vec(),
        ),
        ("GET", "app.js") => response(
            "200 OK",
            "text/javascript; charset=utf-8",
            include_str!("bin/curator.js").as_bytes().to_vec(),
        ),
        ("GET", "palette.js") => response(
            "200 OK",
            "text/javascript; charset=utf-8",
            include_str!("bin/curator_palette.js").as_bytes().to_vec(),
        ),
        ("GET", "pools.js") => response(
            "200 OK",
            "text/javascript; charset=utf-8",
            include_str!("bin/curator_pools.js").as_bytes().to_vec(),
        ),
        ("GET", "catalog") => json_response(catalog_json(state)),
        ("GET", "eligibility") => match query {
            Some(query) => json_response(eligibility_json(state, query)),
            None => json_error("400 Bad Request", "missing preview snapshot query"),
        },
        ("POST", "rules") => match save_rules_request(state, &request.body) {
            Ok(value) => json_response(value),
            Err(error) => json_error("422 Unprocessable Content", &error),
        },
        ("POST", "name") => match save_display_name_request(state, &request.body) {
            Ok(value) => json_response(value),
            Err(error) => json_error("422 Unprocessable Content", &error),
        },
        ("POST", "reload-config") => json_response(reload_config_request()),
        ("GET", path) if path.starts_with("preview/") => {
            let rest = &path["preview/".len()..];
            let mut parts = rest.split('/');
            let (Some(id), Some(kind), None) = (parts.next(), parts.next(), parts.next()) else {
                return not_found();
            };
            match preview_asset(state, id, kind) {
                Ok((content_type, body)) => response("200 OK", content_type, body),
                Err(error) => response(
                    "422 Unprocessable Content",
                    "text/plain; charset=utf-8",
                    error.to_string().into_bytes(),
                ),
            }
        }
        _ => not_found(),
    }
}

fn json_response(value: Value) -> HttpResponse {
    match serde_json::to_vec(&value) {
        Ok(body) => response("200 OK", "application/json; charset=utf-8", body),
        Err(_) => response(
            "500 Internal Server Error",
            "text/plain; charset=utf-8",
            b"serialization failed".to_vec(),
        ),
    }
}

fn json_error(status: &'static str, message: &str) -> HttpResponse {
    let body = serde_json::to_vec(&json!({"ok": false, "error": message}))
        .unwrap_or_else(|_| b"{\"ok\":false}".to_vec());
    response(status, "application/json; charset=utf-8", body)
}

fn catalog_json(state: &AppState) -> Value {
    let scenes = state
        .scenes
        .iter()
        .map(|scene| {
            json!({
                "id": scene.id,
                "title": scene.title,
                "display_name": state.display_name(scene),
                "source": scene.source,
                "source_collection": scene.source_collection,
                "world": scene.world,
                "appearance": scene.appearance,
                "license_state": scene.license_state,
                "cycle_semantics": scene.cycle_semantics,
                "animation_manifest_present": scene.animation.is_some(),
                "time_of_day_present": scene.time_of_day.is_some(),
                "source_cycles_present": scene.source_cycles,
            })
        })
        .collect::<Vec<_>>();
    let editor = state.editor.lock().expect("editor state lock");
    let rules = editor.pools.as_ref().map(|pools| {
        json!({
            "pools": pools.pools().iter().map(|pool| json!({
                "id": pool.id,
                "source": pool.source.as_str(),
                "enabled": pool.enabled,
                "scene_ids": pool.scene_ids,
            })).collect::<Vec<_>>(),
            "scene_rules": pools.rules().iter().map(|(scene_id, rule)| json!({
                "scene_id": scene_id,
                "enabled": rule.enabled,
                "time_of_day": rule.time_of_day.as_ref().map(|times| times.iter().map(|time| scene_pools::time_of_day_name(*time)).collect::<Vec<_>>()),
                "weather_any": rule.weather_any.as_ref().map(|weathers| weathers.iter().map(|weather| scene_pools::weather_name(*weather)).collect::<Vec<_>>()),
                "temperature_min_c": rule.temperature_min_c,
                "temperature_max_c": rule.temperature_max_c,
            })).collect::<Vec<_>>(),
        })
    });
    json!({
        "scenes": scenes,
        "registry_note": state.registry_note,
        "rules": rules,
        "rules_path": editor.rules_path.display().to_string(),
        "rules_present": editor.pools.is_some(),
        "last_save": editor.last_save,
        "message": "Editing changes only this sidecar; Worldwall reloads it only when explicitly requested.",
    })
}

fn preview_environment(query: &str) -> Result<PoolEnvironment, String> {
    let mut time = TimeOfDay::Unknown;
    let mut weather = Weather::Unknown;
    let mut temperature_c = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        match key {
            "time" => {
                time = scene_pools::time_of_day_from_name(value)
                    .ok_or_else(|| "unknown preview time value".to_owned())?;
            }
            "weather" => {
                weather = scene_pools::weather_from_name(value)
                    .ok_or_else(|| "unknown preview weather value".to_owned())?;
            }
            "temperature" => {
                temperature_c = if value == "unknown" {
                    None
                } else {
                    Some(
                        value
                            .parse::<f32>()
                            .map_err(|_| "preview temperature must be a number or unknown")?,
                    )
                };
            }
            _ => {}
        }
    }
    Ok(PoolEnvironment {
        time,
        weather,
        temperature_c,
    })
}

fn eligibility_json(state: &AppState, query: &str) -> Value {
    let environment = match preview_environment(query) {
        Ok(environment) => environment,
        Err(error) => return json!({"ok": false, "error": error}),
    };
    let editor = state.editor.lock().expect("editor state lock");
    let mut known: BTreeSet<&str> = state.scenes.iter().map(|scene| scene.id.as_str()).collect();
    if let Some(pools) = editor.pools.as_ref() {
        for pool in pools.pools() {
            known.extend(pool.scene_ids.iter().map(String::as_str));
        }
    }
    let scenes = known
        .into_iter()
        .map(|id| {
            let (eligible, reasons) = match editor.pools.as_ref() {
                Some(pools) => {
                    let eligibility = pools.evaluate(id, &environment);
                    (
                        eligibility.eligible(),
                        eligibility
                            .reasons
                            .iter()
                            .map(|reason| {
                                json!({
                                    "code": reason.as_str(),
                                    "detail": reason.as_detail(),
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                }
                None => (
                    true,
                    vec![json!({
                        "code": EligibilityReason::NotInPool.as_str(),
                        "detail": "no sidecar is configured; legacy selection is unchanged",
                    })],
                ),
            };
            json!({"id": id, "eligible": eligible, "reasons": reasons})
        })
        .collect::<Vec<_>>();
    json!({
        "ok": true,
        "snapshot": {
            "time": scene_pools::time_of_day_name(environment.time),
            "weather": scene_pools::weather_name(environment.weather),
            "temperature_c": environment.temperature_c,
        },
        "scenes": scenes,
    })
}

fn pools_from_edit_payload(body: &[u8]) -> Result<ScenePools, String> {
    let value: Value = serde_json::from_slice(body).map_err(|_| "rules body must be JSON")?;
    let pools = value
        .get("pools")
        .and_then(Value::as_array)
        .ok_or("rules body needs a pools array")?;
    let mut pool_entries = Vec::with_capacity(pools.len());
    for pool in pools {
        let id = pool
            .get("id")
            .and_then(Value::as_str)
            .ok_or("pool id must be a string")?
            .to_owned();
        let source = match pool.get("source").and_then(Value::as_str) {
            Some("library") => PoolSource::Library,
            Some("registry") => PoolSource::Registry,
            _ => return Err("pool source must be library or registry".into()),
        };
        let enabled = pool
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or("pool enabled must be a boolean")?;
        let mut scene_ids = Vec::new();
        for scene_id in pool
            .get("scene_ids")
            .and_then(Value::as_array)
            .ok_or("pool scene_ids must be an array")?
        {
            scene_ids.push(
                scene_id
                    .as_str()
                    .ok_or("pool scene_ids must be strings")?
                    .to_owned(),
            );
        }
        pool_entries.push(PoolEntry {
            id,
            source,
            enabled,
            scene_ids,
        });
    }
    let mut rules = Vec::new();
    if let Some(rows) = value.get("scene_rules").and_then(Value::as_array) {
        for row in rows {
            let scene_id = row
                .get("scene_id")
                .and_then(Value::as_str)
                .ok_or("scene_rule scene_id must be a string")?
                .to_owned();
            let enabled = row
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or("scene_rule enabled must be a boolean")?;
            let time_of_day = match row.get("time_of_day").and_then(Value::as_array) {
                Some(values) => {
                    let mut times = BTreeSet::new();
                    for value in values {
                        let name = value
                            .as_str()
                            .ok_or("scene_rule time_of_day values must be strings")?;
                        times.insert(
                            scene_pools::time_of_day_from_name(name)
                                .ok_or("scene_rule time_of_day value is unsupported")?,
                        );
                    }
                    if times.is_empty() {
                        return Err("scene_rule time_of_day must not be empty".into());
                    }
                    Some(times)
                }
                None => None,
            };
            let weather_any = match row.get("weather_any").and_then(Value::as_array) {
                Some(values) => {
                    let mut weathers = BTreeSet::new();
                    for value in values {
                        let name = value
                            .as_str()
                            .ok_or("scene_rule weather_any values must be strings")?;
                        weathers.insert(
                            scene_pools::weather_from_name(name)
                                .ok_or("scene_rule weather_any value is unsupported")?,
                        );
                    }
                    if weathers.is_empty() {
                        return Err("scene_rule weather_any must not be empty".into());
                    }
                    Some(weathers)
                }
                None => None,
            };
            let mut temperature_min_c = None;
            let mut temperature_max_c = None;
            for (key, target) in [
                ("temperature_min_c", &mut temperature_min_c),
                ("temperature_max_c", &mut temperature_max_c),
            ] {
                if let Some(value) = row.get(key).and_then(Value::as_f64) {
                    if !value.is_finite() || !(-100.0..=100.0).contains(&value) {
                        return Err(format!("scene_rule {key} is out of range"));
                    }
                    *target = Some(value as f32);
                }
            }
            if matches!(
                (temperature_min_c, temperature_max_c),
                (Some(minimum), Some(maximum)) if minimum > maximum
            ) {
                return Err("scene_rule temperature range is inverted".into());
            }
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
    }
    ScenePools::from_parts(pool_entries, rules).map_err(|error| error.to_string())
}

fn save_rules_request(state: &AppState, body: &[u8]) -> Result<Value, String> {
    let pools = pools_from_edit_payload(body)?;
    let document = pools.to_toml_string();
    let mut editor = state.editor.lock().expect("editor state lock");
    save_rules(&editor.rules_path, &document).map_err(|error| error.to_string())?;
    editor.pools = Some(pools);
    editor.last_save = Some(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default()
            .to_string(),
    );
    Ok(json!({
        "ok": true,
        "saved": editor.rules_path.display().to_string(),
        "message": "Sidecar saved atomically; request config reload explicitly to apply it.",
    }))
}

fn save_display_name_request(state: &AppState, body: &[u8]) -> Result<Value, String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| "name body must be JSON".to_owned())?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or("name body needs a scene id")?;
    let display_name = match value.get("display_name") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) if name.is_empty() => None,
        Some(Value::String(name)) => Some(name.clone()),
        Some(_) => return Err("display_name must be a string or null".into()),
    };
    state
        .scene(id)
        .filter(|scene| scene.source == "library")
        .ok_or("display names are editable for library scenes only")?;
    scene_pools::set_catalog_display_name(&state.library_root, id, display_name.as_deref())
        .map_err(|error| error.to_string())?;
    let mut names = state.display_names.lock().expect("display-name lock");
    match &display_name {
        Some(name) => {
            names.insert(id.to_owned(), name.clone());
        }
        None => {
            names.remove(id);
        }
    }
    Ok(json!({
        "ok": true,
        "message": "Display name saved to the library catalog.",
    }))
}

/// Issue one explicit `reload-config` request to a running Worldwall. This is
/// a user action; nothing else in the curator touches the wallpaper process.
fn reload_config_request() -> Value {
    let path = match ipc::socket_path_from_environment() {
        Ok(path) => path,
        Err(error) => {
            return json!({"ok": false, "error": format!("no control socket is configured: {error}")});
        }
    };
    let mut stream = match UnixStream::connect(&path) {
        Ok(stream) => stream,
        Err(error) => {
            return json!({"ok": false, "error": format!("Worldwall is not reachable: {error}")});
        }
    };
    if let Err(error) = stream.set_read_timeout(Some(ipc::COMMAND_TIMEOUT)) {
        return json!({"ok": false, "error": format!("control socket timeout: {error}")});
    }
    if let Err(error) = ipc::write_request(&mut stream, &Command::ReloadConfig) {
        return json!({"ok": false, "error": format!("control request failed: {error}")});
    }
    let mut length = [0; 4];
    if let Err(error) = stream.read_exact(&mut length) {
        return json!({"ok": false, "error": format!("control response failed: {error}")});
    }
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > ipc::MAX_RESPONSE_BYTES {
        return json!({"ok": false, "error": "control response is out of bounds"});
    }
    let mut body = vec![0; length];
    if let Err(error) = stream.read_exact(&mut body) {
        return json!({"ok": false, "error": format!("control response failed: {error}")});
    }
    match serde_json::from_slice::<Value>(&body) {
        Ok(value) => json!({"ok": true, "response": value}),
        Err(_) => json!({"ok": false, "error": "control response is not JSON"}),
    }
}

fn preview_asset(
    state: &AppState,
    id: &str,
    kind: &str,
) -> Result<(&'static str, Vec<u8>), Box<dyn Error>> {
    let scene = state
        .scene(id)
        .ok_or_else(|| "unknown scene ID".to_owned())?;
    match kind {
        "indices" => {
            let bytes =
                scene_pools::read_regular_nofollow(&scene.image, MAX_PREVIEW_BYTES, "scene PNG")?;
            let decoded = scene::decode_index_png(&bytes)?;
            if u64::from(decoded.width) * u64::from(decoded.height) > MAX_PREVIEW_PIXELS {
                return Err("scene image exceeds the preview pixel bound".into());
            }
            let mut body = Vec::with_capacity(8 + decoded.indices.len());
            body.extend_from_slice(&decoded.width.to_be_bytes());
            body.extend_from_slice(&decoded.height.to_be_bytes());
            body.extend_from_slice(&decoded.indices);
            Ok(("application/octet-stream", body))
        }
        "palette" => {
            let bytes = scene_pools::read_regular_nofollow(
                &scene.palette,
                MAX_PREVIEW_BYTES,
                "palette PNG",
            )?;
            let decoded = scene::decode_palette_png(&bytes)?;
            Ok(("application/octet-stream", decoded.rgba.to_vec()))
        }
        "animation" | "cycle-indices" => {
            // A package without an authored animation manifest previews as the
            // runtime's static default; unmapped source cycles are never
            // synthesized into animation data.
            let animation = match &scene.animation {
                Some(path) => {
                    let bytes = scene_pools::read_regular_nofollow(
                        path,
                        MAX_PREVIEW_BYTES,
                        "animation manifest",
                    )?;
                    let text = std::str::from_utf8(&bytes)?;
                    Animation::from_toml(text)?
                }
                None => Animation::static_default(),
            };
            let body = if kind == "animation" {
                animation.metadata_bytes()
            } else {
                animation.cycle_index_bytes()
            };
            Ok(("application/octet-stream", body))
        }
        "tod-atlas" | "tod-keyframes" | "info" => preview_time_of_day(scene, kind),
        _ => Err("unknown preview asset kind".into()),
    }
}

fn load_registry_scenes() -> Result<Vec<PreviewScene>, Box<dyn Error>> {
    let root = super::configured_registry_dir()?;
    let registry = scene_registry::SceneRegistry::load(&root)?;
    Ok(registry
        .manifests()
        .map(PreviewScene::from_manifest)
        .collect())
}

fn preview_time_of_day(
    scene: &PreviewScene,
    kind: &str,
) -> Result<(&'static str, Vec<u8>), Box<dyn Error>> {
    let timeline = match &scene.time_of_day {
        Some(path) => Some(scene::load_time_of_day(path)?),
        None => None,
    };
    match kind {
        "info" => {
            let image =
                scene_pools::read_regular_nofollow(&scene.image, MAX_PREVIEW_BYTES, "scene PNG")?;
            let decoded = scene::decode_index_png(&image)?;
            let info = json!({
                "width": decoded.width,
                "height": decoded.height,
                "source": scene.source,
                "animation_manifest_present": scene.animation.is_some(),
                "cycle_semantics": scene.cycle_semantics,
                "time_of_day": timeline.as_ref().map(|timeline| json!({
                    "rows": timeline.rows,
                    "keyframe_count": timeline.keyframe_count(),
                    "solar": timeline.solar_keyframes.is_some(),
                })),
            });
            Ok((
                "application/json; charset=utf-8",
                serde_json::to_vec(&info)?,
            ))
        }
        "tod-atlas" => {
            let timeline = timeline.ok_or("scene has no time-of-day atlas")?;
            let mut body = Vec::with_capacity(4 + timeline.rgba.len());
            body.extend_from_slice(&timeline.rows.to_be_bytes());
            body.extend_from_slice(&timeline.rgba);
            Ok(("application/octet-stream", body))
        }
        "tod-keyframes" => {
            let timeline = timeline.ok_or("scene has no time-of-day atlas")?;
            let value = match &timeline.solar_keyframes {
                Some(solar) => json!({
                    "solar": true,
                    "rows": timeline.rows,
                    "keyframes": solar.iter().map(|keyframe| json!({
                        "event": match keyframe.event {
                            scene::SolarEvent::Sunrise => "sunrise",
                            scene::SolarEvent::Sunset => "sunset",
                        },
                        "offset_minutes": keyframe.offset_minutes,
                        "row": keyframe.row,
                    })).collect::<Vec<_>>(),
                }),
                None => json!({
                    "solar": false,
                    "rows": timeline.rows,
                    "keyframes": timeline.keyframes.iter().map(|keyframe| json!({
                        "civil_seconds": keyframe.civil_seconds,
                        "row": keyframe.row,
                    })).collect::<Vec<_>>(),
                }),
            };
            Ok((
                "application/json; charset=utf-8",
                serde_json::to_vec(&value)?,
            ))
        }
        _ => Err("unknown time-of-day preview kind".into()),
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

fn random_token() -> io::Result<String> {
    let mut bytes = [0u8; 24];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use png::{BitDepth, ColorType};

    fn temporary_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "worldwall-curator-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn png_bytes(width: u32, height: u32, color: ColorType, bytes: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, width, height);
            encoder.set_color(color);
            encoder.set_depth(BitDepth::Eight);
            if color == ColorType::Indexed {
                encoder.set_palette(vec![0, 0, 0, 255, 255, 255]);
            }
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(bytes).unwrap();
        }
        encoded
    }

    fn sample_library(root: &Path) {
        let package = root.join("canvascycle-demo");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("scene.png"),
            png_bytes(2, 1, ColorType::Indexed, &[0, 1]),
        )
        .unwrap();
        let mut palette = vec![7u8; 256 * 4];
        for entry in palette.chunks_mut(4) {
            entry[3] = 255;
        }
        fs::write(
            package.join("palette.png"),
            png_bytes(256, 1, ColorType::Rgba, &palette),
        )
        .unwrap();
        fs::write(
            package.join("scene.toml"),
            "schema_version = 1\nid = \"canvascycle-demo\"\nimage = \"scene.png\"\npalette = \"palette.png\"\n",
        )
        .unwrap();
        fs::write(
            package.join("provenance.toml"),
            "schema_version = 1\nsource_format = \"canvascycle-legacy-object-literal-v1\"\n",
        )
        .unwrap();
        let digest = "a".repeat(64);
        fs::write(
            root.join("library.toml"),
            format!(
                "schema_version = 1\n\n[[scene]]\nid = \"canvascycle-demo\"\npackage = \"canvascycle-demo\"\nsource_collection = \"canvascycle\"\nworld = \"unknown\"\nappearance = \"unknown\"\nimage = \"scene.png\"\nimage_sha256 = \"{digest}\"\npalette = \"palette.png\"\npalette_sha256 = \"{digest}\"\nsource_format = \"canvascycle-legacy-object-literal-v1\"\nsource_sha256 = \"{digest}\"\nsource_url = \"https://example.invalid/capture\"\nsource_page = \"https://example.invalid/\"\nsource_title = \"Demo\"\nlicense_state = \"unreviewed\"\n"
            ),
        )
        .unwrap();
    }

    fn sample_state(root: &Path, rules_path: PathBuf) -> AppState {
        let catalog = LibraryCatalog::load(root).unwrap();
        app_state(
            catalog
                .scenes()
                .iter()
                .map(PreviewScene::from_catalog)
                .collect(),
            "".to_owned(),
            rules_path,
        )
    }

    fn app_state(
        scenes: Vec<PreviewScene>,
        registry_note: String,
        rules_path: PathBuf,
    ) -> AppState {
        let mut by_id = HashMap::new();
        for (index, scene) in scenes.iter().enumerate() {
            by_id.entry(scene.id.clone()).or_insert(index);
        }
        AppState {
            scenes,
            by_id,
            library_root: rules_path.clone(),
            display_names: Mutex::new(HashMap::new()),
            registry_note,
            editor: Mutex::new(EditorState {
                pools: None,
                rules_path,
                last_save: None,
            }),
            token: "0123".into(),
        }
    }

    #[test]
    fn editing_payloads_validate_and_save_atomically_with_private_permissions() {
        let root = temporary_root("save");
        sample_library(&root);
        let rules_path = root.join("config").join("scene-pools.toml");
        let state = sample_state(&root, rules_path.clone());
        let body = br#"{"pools":[{"id":"canvascycle","source":"library","enabled":true,"scene_ids":["canvascycle-demo"]}],"scene_rules":[]}"#;
        let value = save_rules_request(&state, body).unwrap();
        assert_eq!(value["ok"], Value::Bool(true));
        let saved = fs::read_to_string(&rules_path).unwrap();
        assert!(saved.contains("id = \"canvascycle\""));
        assert!(ScenePools::parse(&saved).is_ok(), "saved sidecar is valid");
        let mode = fs::symlink_metadata(&rules_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "sidecar is same-user readable/writable only");
        assert!(state.editor.lock().unwrap().pools.is_some());

        // An invalid edit leaves the previous valid sidecar untouched.
        let before = fs::read_to_string(&rules_path).unwrap();
        let bad = br#"{"pools":[{"id":"a","source":"library","enabled":true,"scene_ids":["x"]},{"id":"b","source":"registry","enabled":true,"scene_ids":["x"]}],"scene_rules":[]}"#;
        assert!(save_rules_request(&state, bad).is_err());
        assert_eq!(fs::read_to_string(&rules_path).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn eligibility_uses_one_preview_snapshot_and_reports_reasons() {
        let root = temporary_root("eligibility");
        sample_library(&root);
        let rules_path = root.join("scene-pools.toml");
        let state = sample_state(&root, rules_path);
        let body = br#"{"pools":[{"id":"canvascycle","source":"library","enabled":true,"scene_ids":["canvascycle-demo"]}],"scene_rules":[{"scene_id":"canvascycle-demo","enabled":true,"time_of_day":["day"],"temperature_min_c":10.0,"temperature_max_c":30.0,"weather_any":["clear"]}]}"#;
        save_rules_request(&state, body).unwrap();

        let eligible = eligibility_json(&state, "time=day&weather=clear&temperature=20");
        assert_eq!(eligible["ok"], Value::Bool(true));
        assert_eq!(eligible["scenes"][0]["eligible"], Value::Bool(true));

        let mismatch = eligibility_json(&state, "time=night&weather=rain&temperature=unknown");
        let reasons = mismatch["scenes"][0]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|reason| reason["code"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            reasons,
            [
                "time-of-day-mismatch",
                "weather-mismatch",
                "temperature-unknown"
            ]
        );
        assert_eq!(mismatch["scenes"][0]["eligible"], Value::Bool(false));

        let invalid = eligibility_json(&state, "time=sunset&weather=clear&temperature=20");
        assert_eq!(invalid["ok"], Value::Bool(false));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_routes_serve_real_indexed_pixels_palette_and_animation_metadata() {
        let root = temporary_root("preview");
        sample_library(&root);
        let state = sample_state(&root, root.join("scene-pools.toml"));

        let (_, indices) = preview_asset(&state, "canvascycle-demo", "indices").unwrap();
        assert_eq!(&indices[..4], &2u32.to_be_bytes(), "width framing");
        assert_eq!(&indices[4..8], &1u32.to_be_bytes(), "height framing");
        assert_eq!(&indices[8..], &[0, 1], "raw decoded indices");

        let (_, palette) = preview_asset(&state, "canvascycle-demo", "palette").unwrap();
        assert_eq!(palette.len(), 256 * 4);

        // No authored animation: the preview serves the runtime's static
        // default (all slots effect=0) and never fabricates cycle data.
        let (_, metadata) = preview_asset(&state, "canvascycle-demo", "animation").unwrap();
        assert_eq!(metadata.len(), 256 * 32, "one metadata entry per slot");
        assert!(
            metadata
                .chunks(32)
                .all(|entry| entry[0..4] == 0u32.to_ne_bytes()),
            "static default metadata carries no effects"
        );
        let (_, cycles) = preview_asset(&state, "canvascycle-demo", "cycle-indices").unwrap();
        assert_eq!(cycles.len(), 256 * 4);
        assert!(preview_asset(&state, "missing", "indices").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn registry_scenes_browse_with_source_marker_and_authored_animation() {
        let root = temporary_root("registry-scenes");
        let package = root.join("alpha");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("scene.png"),
            png_bytes(2, 1, ColorType::Indexed, &[0, 1]),
        )
        .unwrap();
        let mut palette = vec![7u8; 256 * 4];
        for entry in palette.chunks_mut(4) {
            entry[3] = 255;
        }
        fs::write(
            package.join("palette.png"),
            png_bytes(256, 1, ColorType::Rgba, &palette),
        )
        .unwrap();
        fs::write(
            package.join("scene.toml"),
            "schema_version = 1\nid = \"alpha\"\nimage = \"scene.png\"\npalette = \"palette.png\"\nanimation = \"animation.toml\"\ntime_of_day = \"time_of_day.toml\"\n",
        )
        .unwrap();
        fs::write(
            package.join("animation.toml"),
            "schema_version = 1\n\n[[group]]\nname = \"g\"\neffect = \"hue_cycle\"\nrange = [0, 3]\nspeed = 0.5\namplitude = 1.0\nphase = 0.0\n",
        )
        .unwrap();
        fs::write(
            package.join("time_of_day.toml"),
            "schema_version = 1\natlas = \"time_of_day.png\"\n\n[[keyframe]]\ncivil_seconds = 0\nrow = 0\n\n[[keyframe]]\ncivil_seconds = 43200\nrow = 1\n",
        )
        .unwrap();
        let mut atlas = vec![9u8; 256 * 2 * 4];
        for entry in atlas.chunks_mut(4) {
            entry[3] = 255;
        }
        fs::write(
            package.join("time_of_day.png"),
            png_bytes(256, 2, ColorType::Rgba, &atlas),
        )
        .unwrap();
        fs::write(
            root.join("registry.toml"),
            "schema_version = 1\nscenes = [\"alpha\"]\nfallback = \"alpha\"\n",
        )
        .unwrap();

        let registry = scene_registry::SceneRegistry::load(&root).unwrap();
        let scenes = registry
            .manifests()
            .map(PreviewScene::from_manifest)
            .collect::<Vec<_>>();
        assert_eq!(scenes[0].source, "registry");
        assert!(scenes[0].animation.is_some());
        let state = app_state(
            scenes,
            "registry loaded".to_owned(),
            root.join("rules.toml"),
        );

        // Registry scenes are browsable with their authored animation and
        // time-of-day atlas through the same preview routes.
        let catalog = catalog_json(&state);
        assert_eq!(catalog["registry_note"], "registry loaded");
        assert_eq!(catalog["scenes"][0]["source"], "registry");
        assert_eq!(catalog["scenes"][0]["animation_manifest_present"], true);
        assert_eq!(catalog["scenes"][0]["license_state"], "not-recorded");
        let (_, metadata) = preview_asset(&state, "alpha", "animation").unwrap();
        assert_eq!(metadata.len(), 256 * 32);
        assert!(
            metadata
                .chunks(32)
                .take(4)
                .any(|entry| entry[0..4] != 0u32.to_ne_bytes()),
            "authored effects reach the preview payload"
        );
        let (_, atlas) = preview_asset(&state, "alpha", "tod-atlas").unwrap();
        assert_eq!(&atlas[..4], &2u32.to_be_bytes());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preview_rejects_non_indexed_and_transparent_png_inputs() {
        let root = temporary_root("preview-invalid");
        let package = root.join("package");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("scene.png"),
            png_bytes(1, 1, ColorType::Rgba, &[0, 0, 0, 255]),
        )
        .unwrap();
        fs::write(
            package.join("palette.png"),
            png_bytes(256, 1, ColorType::Rgba, &vec![0; 256 * 4]),
        )
        .unwrap();
        let bytes =
            scene_pools::read_regular_nofollow(&package.join("scene.png"), MAX_PREVIEW_BYTES, "x")
                .unwrap();
        assert!(scene::decode_index_png(&bytes).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn http_requests_accept_get_and_post_and_reject_oversized_bodies() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(address).unwrap();
        client
            .write_all(b"POST /x/rules HTTP/1.1\r\nContent-Length: 4\r\n\r\nabcd")
            .unwrap();
        let (server, _) = listener.accept().unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = read_request(&server).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/x/rules");
        assert_eq!(request.body, b"abcd");
        drop(server);
        drop(client);

        let mut client = TcpStream::connect(address).unwrap();
        client
            .write_all(b"POST /x/rules HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n")
            .unwrap();
        let (server, _) = listener.accept().unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert!(
            read_request(&server).is_err(),
            "oversized bodies are rejected"
        );
    }
}
