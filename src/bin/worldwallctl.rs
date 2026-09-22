//! Command-line client for Worldwall's bounded local control socket.

#[path = "../ipc.rs"]
mod ipc;

use std::{env, error::Error, io, os::unix::net::UnixStream, path::PathBuf};

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

fn parse_command(args: &[String]) -> Result<ipc::Command, Box<dyn Error>> {
    match args {
        [command] if command == "status" => Ok(ipc::Command::Status),
        [command] if command == "reload" => Ok(ipc::Command::Reload),
        [command] if command == "reload-config" => Ok(ipc::Command::ReloadConfig),
        [command] if command == "next" => Ok(ipc::Command::Next),
        [command] if command == "weather" => Ok(ipc::Command::Weather),
        [command, id] if command == "scene" => Ok(ipc::Command::Scene(id.clone())),
        _ => Err(
            "usage: worldwallctl [--socket PATH] status|reload|reload-config|scene ID|next|weather"
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
