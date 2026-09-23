//! Command-line client for Worldwall's bounded local control socket.

#[path = "../ipc.rs"]
mod ipc;

use std::{
    collections::HashSet,
    env,
    error::Error,
    fs, io,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("worldwallctl: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mut socket = None;
    let mut command_args = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--socket" {
            socket = Some(PathBuf::from(
                args.next().ok_or("--socket requires a path")?,
            ));
        } else {
            command_args.push(arg);
        }
    }
    if matches!(command_args.as_slice(), [command] if command == "scenes" || command == "list") {
        return list_scenes(socket.as_deref());
    }
    let command = parse_command(&command_args)?;
    let path = socket.unwrap_or(ipc::socket_path_from_environment()?);
    let mut stream = UnixStream::connect(&path)
        .map_err(|error| format!("connecting to {} failed: {error}", path.display()))?;
    stream.set_read_timeout(Some(ipc::COMMAND_TIMEOUT))?;
    stream.set_write_timeout(Some(ipc::COMMAND_TIMEOUT))?;
    ipc::write_request(&mut stream, &command)?;
    let response = read_response(&mut stream)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(io::Error::other("Worldwall rejected the command").into());
    }
    Ok(())
}

fn list_scenes(socket: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let registry = configured_registry_dir()?;
    let scenes = read_scene_ids(&registry)?;
    let active = match socket {
        Some(path) => active_scene_at(path),
        None => ipc::socket_path_from_environment()
            .ok()
            .and_then(|path| active_scene_at(&path)),
    };
    println!("Registered scenes in {}:", registry.display());
    if active.is_none() {
        println!("  (Worldwall is not reachable; no active scene marked)");
    }
    for scene in scenes {
        let marker = if active.as_deref() == Some(scene.as_str()) {
            "* "
        } else {
            "  "
        };
        println!("{marker}{scene}");
    }
    Ok(())
}

fn configured_registry_dir() -> Result<PathBuf, Box<dyn Error>> {
    let explicit_config = env::var_os("WORLDWALL_CONFIG").map(PathBuf::from);
    let default_config = || {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("HOME is not set")?;
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        Ok::<PathBuf, Box<dyn Error>>(config_home.join("worldwall/worldwall.toml"))
    };
    let config_path = match explicit_config {
        Some(path) => Some(path),
        None => {
            let path = default_config()?;
            path.exists().then_some(path)
        }
    };

    let registry = if let Some(config_path) = config_path {
        let text = fs::read_to_string(&config_path)?;
        let config: toml::Value = toml::from_str(&text)?;
        let registry = config
            .get("registry_dir")
            .and_then(toml::Value::as_str)
            .ok_or("Worldwall config has no string registry_dir")?;
        if registry == "none" {
            return Err("the selected Worldwall config disables scene registry selection".into());
        }
        PathBuf::from(registry)
    } else if let Some(registry) = env::var_os("WORLDWALL_REGISTRY_DIR") {
        PathBuf::from(registry)
    } else {
        return Err("no Worldwall config or WORLDWALL_REGISTRY_DIR was found".into());
    };
    let registry = fs::canonicalize(registry)?;
    if !registry.is_dir() {
        return Err(format!("scene registry is not a directory: {}", registry.display()).into());
    }
    Ok(registry)
}

fn read_scene_ids(registry: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let text = fs::read_to_string(registry.join("registry.toml"))?;
    let value: toml::Value = toml::from_str(&text)?;
    let entries = value
        .get("scenes")
        .and_then(toml::Value::as_array)
        .ok_or("registry.toml must contain a scenes array")?;
    let mut seen = HashSet::new();
    let mut scenes = Vec::with_capacity(entries.len());
    for entry in entries {
        let scene = entry.as_str().ok_or("registry scene IDs must be strings")?;
        if scene.is_empty()
            || !scene
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(format!("registry contains an unsafe scene ID: {scene:?}").into());
        }
        if !seen.insert(scene.to_owned()) {
            return Err(format!("registry contains a duplicate scene ID: {scene}").into());
        }
        scenes.push(scene.to_owned());
    }
    Ok(scenes)
}

fn active_scene_at(path: &Path) -> Option<String> {
    let mut stream = UnixStream::connect(path).ok()?;
    stream.set_read_timeout(Some(ipc::COMMAND_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(ipc::COMMAND_TIMEOUT)).ok()?;
    ipc::write_request(&mut stream, &ipc::Command::Status).ok()?;
    let response = read_response(&mut stream).ok()?;
    if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }
    response
        .get("data")
        .and_then(|data| data.get("scene"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn parse_command(args: &[String]) -> Result<ipc::Command, Box<dyn Error>> {
    match args {
        [command] if command == "status" => Ok(ipc::Command::Status),
        [command] if command == "reload" => Ok(ipc::Command::Reload),
        [command] if command == "reload-config" => Ok(ipc::Command::ReloadConfig),
        [command] if command == "next" => Ok(ipc::Command::Next),
        [command] if command == "weather" => Ok(ipc::Command::Weather),
        [command, id] if command == "scene" => Ok(ipc::Command::Scene(id.clone())),
        _ => Err(
            "usage: worldwallctl [--socket PATH] status|scenes|reload|reload-config|scene ID|next|weather"
                .into(),
        ),
    }
}

fn read_response(reader: &mut impl io::Read) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > ipc::MAX_RESPONSE_BYTES {
        return Err("response exceeds the permitted framed size".into());
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

#[cfg(test)]
mod tests {
    use super::read_scene_ids;
    use std::{fs, path::PathBuf, time::SystemTime};

    fn registry_file(contents: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("worldwallctl-scenes-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("registry.toml"), contents).unwrap();
        root
    }

    #[test]
    fn reads_scene_ids_in_registry_order() {
        let root = registry_file(
            "schema_version = 1\nscenes = [\"v29-seascape\", \"v26-winter-forest\"]\n",
        );
        let scenes = read_scene_ids(&root).unwrap();
        fs::remove_dir_all(root).unwrap();
        assert_eq!(scenes, ["v29-seascape", "v26-winter-forest"]);
    }

    #[test]
    fn rejects_unsafe_or_duplicate_scene_ids() {
        let unsafe_root = registry_file("scenes = [\"../outside\"]\n");
        assert!(read_scene_ids(&unsafe_root).is_err());
        fs::remove_dir_all(unsafe_root).unwrap();

        let duplicate_root = registry_file("scenes = [\"scene\", \"scene\"]\n");
        assert!(read_scene_ids(&duplicate_root).is_err());
        fs::remove_dir_all(duplicate_root).unwrap();
    }
}
