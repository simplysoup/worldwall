//! Bounded same-user Unix-socket protocol for local Worldwall controls.
#![allow(dead_code)]
//!
//! The listener owns no Wayland, GPU, or scene state. It parses and bounds
//! requests on worker threads, then sends a typed command to the event-loop
//! owner. Only the event-loop owner may apply a command or produce its result.

use std::{
    error::Error,
    fmt, fs,
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use calloop::channel::{self, Channel};
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_REQUEST_BYTES: usize = 4 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024;
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CLIENTS: usize = 16;
const SOCKET_MODE: u32 = 0o600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Status,
    Reload,
    ReloadConfig,
    Scene(String),
    Next,
    Weather,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError(String);

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ProtocolError {}

fn protocol_error(message: impl Into<String>) -> ProtocolError {
    ProtocolError(message.into())
}

pub struct ControlRequest {
    pub command: Command,
    response: mpsc::SyncSender<Value>,
}

impl ControlRequest {
    pub fn respond(self, response: Value) {
        let _ = self.response.try_send(response);
    }
}

pub struct IpcServer {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    clients: Arc<AtomicUsize>,
}

impl IpcServer {
    pub fn start_default() -> Result<(Self, Channel<ControlRequest>), Box<dyn Error>> {
        let path = socket_path_from_environment()?;
        Self::start(path)
    }

    pub fn start(
        path: impl Into<PathBuf>,
    ) -> Result<(Self, Channel<ControlRequest>), Box<dyn Error>> {
        let path = path.into();
        validate_socket_parent(&path)?;
        prepare_socket_path(&path)?;
        let listener = UnixListener::bind(&path).map_err(|error| {
            format!(
                "binding Worldwall control socket {} failed: {error}",
                path.display()
            )
        })?;
        fs::set_permissions(&path, fs::Permissions::from_mode(SOCKET_MODE))?;
        let (sender, channel) = channel::sync_channel(32);
        let stop = Arc::new(AtomicBool::new(false));
        let clients = Arc::new(AtomicUsize::new(0));
        let thread_stop = Arc::clone(&stop);
        let thread_clients = Arc::clone(&clients);
        thread::Builder::new()
            .name("worldwall-ipc-accept".to_owned())
            .spawn(move || accept_loop(listener, sender, thread_stop, thread_clients))?;
        println!(
            "trace ipc-listening path={} protocol={} mode=0600",
            path.display(),
            PROTOCOL_VERSION
        );
        Ok((
            Self {
                path,
                stop,
                clients,
            },
            channel,
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Only remove a socket that is still owned by this user. The path was
        // checked before bind and no other user can replace it in a validated
        // user-owned runtime directory without changing ownership.
        if fs::symlink_metadata(&self.path)
            .ok()
            .is_some_and(|metadata| {
                metadata.uid() == current_uid() && metadata.file_type().is_socket()
            })
        {
            let _ = fs::remove_file(&self.path);
        }
        let _ = self.clients.load(Ordering::Relaxed);
    }
}

fn accept_loop(
    listener: UnixListener,
    sender: channel::SyncSender<ControlRequest>,
    stop: Arc<AtomicBool>,
    clients: Arc<AtomicUsize>,
) {
    let _ = listener.set_nonblocking(true);
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                if peer_uid(&stream).ok() != Some(current_uid()) {
                    eprintln!("worldwall: rejected IPC peer with foreign credentials");
                    continue;
                }
                let previous = clients.fetch_add(1, Ordering::AcqRel);
                if previous >= MAX_CLIENTS {
                    clients.fetch_sub(1, Ordering::AcqRel);
                    eprintln!("worldwall: rejected IPC client limit");
                    continue;
                }
                let thread_sender = sender.clone();
                let thread_clients = Arc::clone(&clients);
                let result = thread::Builder::new()
                    .name("worldwall-ipc-client".to_owned())
                    .spawn(move || {
                        handle_client(stream, thread_sender);
                        thread_clients.fetch_sub(1, Ordering::AcqRel);
                    });
                if result.is_err() {
                    clients.fetch_sub(1, Ordering::AcqRel);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                if !stop.load(Ordering::Acquire) {
                    eprintln!("worldwall: IPC accept failed: {error}");
                }
                break;
            }
        }
    }
}

fn handle_client(mut stream: UnixStream, sender: channel::SyncSender<ControlRequest>) {
    let _ = stream.set_read_timeout(Some(COMMAND_TIMEOUT));
    let _ = stream.set_write_timeout(Some(COMMAND_TIMEOUT));
    let result = read_request(&mut stream)
        .and_then(|command| {
            let (response_sender, response_receiver) = mpsc::sync_channel(1);
            sender
                .try_send(ControlRequest {
                    command,
                    response: response_sender,
                })
                .map_err(|_| protocol_error("Worldwall command queue is full"))?;
            response_receiver
                .recv_timeout(COMMAND_TIMEOUT)
                .map_err(|_| protocol_error("Worldwall command timed out"))
        })
        .and_then(|response| write_response(&mut stream, &response));
    if let Err(error) = result {
        let response = error_response("protocol_error", error.to_string());
        let _ = write_response(&mut stream, &response);
    }
}

pub fn read_request(reader: &mut impl Read) -> Result<Command, ProtocolError> {
    let length = read_length(reader)? as usize;
    if length == 0 || length > MAX_REQUEST_BYTES {
        return Err(protocol_error("request exceeds the permitted framed size"));
    }
    let mut bytes = vec![0; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| protocol_error("request body is truncated"))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| protocol_error("request body is not valid JSON"))?;
    parse_command(&value)
}

pub fn write_request(writer: &mut impl Write, command: &Command) -> io::Result<()> {
    let value = command_value(command);
    write_frame(writer, &value, MAX_REQUEST_BYTES)
}

fn write_response(writer: &mut impl Write, response: &Value) -> Result<(), ProtocolError> {
    write_frame(writer, response, MAX_RESPONSE_BYTES)
        .map_err(|_| protocol_error("response could not be written"))
}

fn write_frame(writer: &mut impl Write, value: &Value, max: usize) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.is_empty() || bytes.len() > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds protocol limit",
        ));
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)
}

fn read_length(reader: &mut impl Read) -> Result<u32, ProtocolError> {
    let mut bytes = [0; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| protocol_error("request frame header is truncated"))?;
    Ok(u32::from_be_bytes(bytes))
}

fn parse_command(value: &Value) -> Result<Command, ProtocolError> {
    let object = value
        .as_object()
        .ok_or_else(|| protocol_error("request must be a JSON object"))?;
    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| protocol_error("request version is required"))?;
    if version != u64::from(PROTOCOL_VERSION) {
        return Err(protocol_error(format!(
            "unsupported protocol version {version}"
        )));
    }
    let name = object
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_error("request command is required"))?;
    match name {
        "status" => Ok(Command::Status),
        "reload" => Ok(Command::Reload),
        "reload-config" => Ok(Command::ReloadConfig),
        "next" => Ok(Command::Next),
        "weather" => Ok(Command::Weather),
        "scene" => {
            let id = object
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| protocol_error("scene command requires an id"))?;
            if id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                return Err(protocol_error("scene id is not a safe identifier"));
            }
            Ok(Command::Scene(id.to_owned()))
        }
        _ => Err(protocol_error(format!("unknown command {name:?}"))),
    }
}

fn command_value(command: &Command) -> Value {
    match command {
        Command::Status => json!({"version": PROTOCOL_VERSION, "command": "status"}),
        Command::Reload => json!({"version": PROTOCOL_VERSION, "command": "reload"}),
        Command::ReloadConfig => json!({"version": PROTOCOL_VERSION, "command": "reload-config"}),
        Command::Scene(id) => json!({"version": PROTOCOL_VERSION, "command": "scene", "id": id}),
        Command::Next => json!({"version": PROTOCOL_VERSION, "command": "next"}),
        Command::Weather => json!({"version": PROTOCOL_VERSION, "command": "weather"}),
    }
}

pub fn success_response(command: &str, data: Value) -> Value {
    json!({"version": PROTOCOL_VERSION, "ok": true, "command": command, "data": data})
}

pub fn error_response(code: &str, message: impl Into<String>) -> Value {
    json!({"version": PROTOCOL_VERSION, "ok": false, "error": {"code": code, "message": message.into()}})
}

pub fn socket_path_from_environment() -> Result<PathBuf, ProtocolError> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(format!("/run/user/{}", current_uid()))))
        .ok_or_else(|| protocol_error("XDG_RUNTIME_DIR is not configured"))?;
    Ok(runtime.join("worldwall.sock"))
}

fn validate_socket_parent(path: &Path) -> Result<(), ProtocolError> {
    let parent = path
        .parent()
        .ok_or_else(|| protocol_error("control socket has no parent directory"))?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| protocol_error(format!("runtime directory is unavailable: {error}")))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(protocol_error("runtime directory must be a real directory"));
    }
    if metadata.uid() != current_uid() {
        return Err(protocol_error(
            "runtime directory is not owned by the current user",
        ));
    }
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(protocol_error(
            "runtime directory is writable by group or other users",
        ));
    }
    Ok(())
}

fn prepare_socket_path(path: &Path) -> Result<(), ProtocolError> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() {
        return Err(protocol_error("existing control path is not a safe socket"));
    }
    if metadata.uid() != current_uid() {
        return Err(protocol_error(
            "existing control socket is owned by another user",
        ));
    }
    match UnixStream::connect(path) {
        Ok(_) => Err(protocol_error("Worldwall is already running")),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            fs::remove_file(path).map_err(|remove_error| {
                protocol_error(format!(
                    "removing stale control socket failed: {remove_error}"
                ))
            })
        }
        Err(error) => Err(protocol_error(format!(
            "existing control socket could not be checked: {error}"
        ))),
    }
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    use std::os::fd::AsRawFd;
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: credentials and length point to writable values of the sizes
    // advertised to getsockopt, and the descriptor is borrowed from stream.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result == 0 {
        Ok(credentials.uid)
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn peer_uid(_stream: &UnixStream) -> io::Result<u32> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "peer credentials are unavailable",
    ))
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions and returns the process uid.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn commands_use_versioned_bounded_frames() {
        let mut frame = Vec::new();
        write_request(&mut frame, &Command::Scene("night-1".into())).unwrap();
        assert_eq!(
            read_request(&mut Cursor::new(frame)),
            Ok(Command::Scene("night-1".into()))
        );
    }

    #[test]
    fn malformed_and_oversized_requests_are_rejected() {
        let mut malformed = Vec::new();
        malformed.extend_from_slice(&3_u32.to_be_bytes());
        malformed.extend_from_slice(b"{}?");
        assert!(read_request(&mut Cursor::new(malformed)).is_err());
        let mut oversized = Vec::new();
        oversized.extend_from_slice(&((MAX_REQUEST_BYTES + 1) as u32).to_be_bytes());
        assert!(read_request(&mut Cursor::new(oversized)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn live_socket_rejects_double_start_and_cleans_its_owned_path() {
        let root = std::env::temp_dir().join(format!("worldwall-s10-ipc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.join("worldwall.sock");
        let (server, _source) = IpcServer::start(&path).unwrap();
        assert!(IpcServer::start(&path).is_err());
        drop(server);
        for _ in 0..20 {
            if !path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn all_public_command_forms_have_stable_parsing() {
        for (body, expected) in [
            (r#"{"version":1,"command":"status"}"#, Command::Status),
            (r#"{"version":1,"command":"reload"}"#, Command::Reload),
            (
                r#"{"version":1,"command":"reload-config"}"#,
                Command::ReloadConfig,
            ),
            (r#"{"version":1,"command":"next"}"#, Command::Next),
            (r#"{"version":1,"command":"weather"}"#, Command::Weather),
            (
                r#"{"version":1,"command":"scene","id":"day"}"#,
                Command::Scene("day".into()),
            ),
        ] {
            let mut frame = Vec::new();
            frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
            frame.extend_from_slice(body.as_bytes());
            assert_eq!(read_request(&mut Cursor::new(frame)), Ok(expected));
        }
    }
}
