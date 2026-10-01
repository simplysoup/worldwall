//! Command-line client for Worldwall's bounded local control socket.

#[allow(dead_code)]
#[path = "../animation.rs"]
mod animation;
#[path = "../curator.rs"]
mod curator;
#[path = "../environment.rs"]
mod environment;
#[path = "../ipc.rs"]
mod ipc;
#[allow(dead_code)]
#[path = "../scene.rs"]
mod scene;
#[path = "../scene_pools.rs"]
mod scene_pools;
#[path = "../scene_registry.rs"]
mod scene_registry;

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
    if command_args
        .first()
        .is_some_and(|command| command == "curator")
    {
        if socket.is_some() {
            return Err("--socket cannot be used with curator".into());
        }
        return curator::run(&command_args[1..]);
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
    let active = match socket {
        Some(path) => active_scene_at(path),
        None => ipc::socket_path_from_environment()
            .ok()
            .and_then(|path| active_scene_at(&path)),
    };
    let mut listed = false;

    match configured_registry_dir()
        .and_then(|registry| Ok((registry.clone(), read_scene_ids(&registry)?)))
    {
        Ok((registry, scenes)) => {
            println!("Registry scenes in {}:", registry.display());
            for scene in scenes {
                let marker = if active.as_deref() == Some(scene.as_str()) {
                    "* "
                } else {
                    "  "
                };
                println!("{marker}{scene}");
            }
            listed = true;
        }
        Err(error) => println!("Registry scenes unavailable: {error}"),
    }

    match default_library_catalog() {
        Ok(catalog) => {
            println!("Library scenes in {}:", catalog.root().display());
            for entry in catalog.scenes() {
                let marker = if active.as_deref() == Some(entry.id.as_str()) {
                    "* "
                } else {
                    "  "
                };
                let name = entry
                    .display_name
                    .as_deref()
                    .unwrap_or(entry.title.as_str());
                println!("{marker}{}  {}", entry.id, name);
            }
            listed = true;
        }
        Err(error) => println!("Library scenes unavailable: {error}"),
    }

    if !listed {
        return Err("no registry or library scenes are available".into());
    }
    if active.is_none() {
        println!("(Worldwall is not reachable; no active scene marked)");
    }
    Ok(())
}

/// The curator's default library root (XDG data home with the documented
/// fallback); `worldwallctl scenes` lists the same catalog the curator edits.
fn default_library_catalog() -> Result<scene_pools::LibraryCatalog, Box<dyn Error>> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    Ok(scene_pools::LibraryCatalog::load(
        &data_home.join("worldwall/scene-library"),
    )?)
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
        [command, mode] if command == "adaptive" => {
            let mode = match mode.as_str() {
                "status" => ipc::AdaptiveModeCommand::Status,
                "on" => ipc::AdaptiveModeCommand::On,
                "off" => ipc::AdaptiveModeCommand::Off,
                _ => return Err("adaptive mode must be status, on, or off".into()),
            };
            Ok(ipc::Command::Adaptive(mode))
        }
        [command, id] if command == "scene" => Ok(ipc::Command::Scene(id.clone())),
        _ => Err(
            "usage: worldwallctl [--socket PATH] status|scenes|reload|reload-config|scene ID|next|weather|adaptive status|on|off; worldwallctl curator [--library DIR] [--rules FILE]"
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
    use super::{ipc, parse_command, read_scene_ids};
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
    fn adaptive_cli_forms_map_to_the_versioned_ipc_command() {
        for (args, expected) in [
            (
                vec!["adaptive".to_owned(), "status".to_owned()],
                ipc::AdaptiveModeCommand::Status,
            ),
            (
                vec!["adaptive".to_owned(), "on".to_owned()],
                ipc::AdaptiveModeCommand::On,
            ),
            (
                vec!["adaptive".to_owned(), "off".to_owned()],
                ipc::AdaptiveModeCommand::Off,
            ),
        ] {
            assert_eq!(
                parse_command(&args).unwrap(),
                ipc::Command::Adaptive(expected)
            );
        }
        assert!(parse_command(&["adaptive".into(), "toggle".into()]).is_err());
        assert!(parse_command(&["adaptive".into()]).is_err());
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
