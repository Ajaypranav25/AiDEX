//! Versioned, loopback-only daemon protocol. Its random endpoint capability is
//! Events are replayed from the durable log on each bounded request; polling by
//! acknowledged cursor avoids a read/subscribe handoff race across disconnects.
use aidex_events::{Event, EventLog, MAX_BATCH};
use serde::{Deserialize, Serialize};

use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[cfg(windows)]
use std::process::Command;

pub const CONTRACT_VERSION: u32 = 1;
const STATE_DIR: &str = ".aidex/runtime";
const STATE_FILE: &str = "daemon.json";
const EVENT_FILE: &str = "events.sqlite";
const MAX_REQUEST_BYTES: u64 = 4096;
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;
const MAX_EVENT_BATCH: usize = MAX_BATCH;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub version: u32,
    pub pid: u32,
    pub port: u16,
    pub token: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    token: String,
    command: String,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    after: Option<u64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Response {
    version: u32,
    ok: bool,
    state: String,
    error: Option<String>,
}

#[derive(Debug)]
pub enum EventRequestError {
    Unavailable(String),
    Rejected(String),
}
impl std::fmt::Display for EventRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) | Self::Rejected(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for EventRequestError {}

pub fn state_path(root: &Path) -> PathBuf {
    root.join(STATE_DIR).join(STATE_FILE)
}

pub fn read_endpoint(root: &Path) -> Result<Endpoint, String> {
    let bytes =
        fs::read(state_path(root)).map_err(|e| format!("daemon endpoint unavailable: {e}"))?;
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err("daemon endpoint is oversized".into());
    }
    let mut endpoint: Endpoint =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid daemon endpoint: {e}"))?;
    if endpoint.version != CONTRACT_VERSION || endpoint.port == 0 {
        return Err("incompatible or invalid daemon endpoint".into());
    }
    endpoint.token = unprotect_token(&endpoint.token)?;
    if endpoint.token.len() != 64 || !endpoint.token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("incompatible or invalid daemon endpoint credential".into());
    }
    Ok(endpoint)
}

fn encode_endpoint(endpoint: &Endpoint) -> Result<Vec<u8>, String> {
    let mut stored = endpoint.clone();
    stored.token = protect_token(&endpoint.token)?;
    serde_json::to_vec(&stored).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn protect_token(token: &str) -> Result<String, String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-Type -AssemblyName System.Security; $plain = [System.Text.Encoding]::UTF8.GetBytes($env:AIDEX_DAEMON_TOKEN); $protected = [System.Security.Cryptography.ProtectedData]::Protect($plain, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser); [Console]::Out.Write([Convert]::ToBase64String($protected))",
        ])
        .env("AIDEX_DAEMON_TOKEN", token)
        .output()
        .map_err(|error| format!("cannot protect daemon credential with Windows DPAPI: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot protect daemon credential with Windows DPAPI: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let protected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if protected.is_empty() || protected == token {
        return Err("Windows DPAPI returned an invalid protected credential".into());
    }
    Ok(protected)
}

#[cfg(windows)]
fn unprotect_token(protected: &str) -> Result<String, String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-Type -AssemblyName System.Security; $protected = [Convert]::FromBase64String($env:AIDEX_DAEMON_TOKEN); $plain = [System.Security.Cryptography.ProtectedData]::Unprotect($protected, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser); [Console]::Out.Write([System.Text.Encoding]::UTF8.GetString($plain))",
        ])
        .env("AIDEX_DAEMON_TOKEN", protected)
        .output()
        .map_err(|error| format!("cannot unlock daemon credential with Windows DPAPI: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot unlock daemon credential with Windows DPAPI: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(windows))]
fn protect_token(token: &str) -> Result<String, String> {
    Ok(token.to_owned())
}

#[cfg(not(windows))]
fn unprotect_token(token: &str) -> Result<String, String> {
    Ok(token.to_owned())
}

pub fn request(root: &Path, command: &str) -> Result<String, String> {
    request_raw(
        root,
        Request {
            version: CONTRACT_VERSION,
            token: String::new(),
            command: command.to_owned(),
            workspace_id: None,
            after: None,
            limit: None,
        },
    )
    .map(|response| response.state)
    .map_err(|error| error.to_string())
}

/// Fetch one bounded replay batch after the caller's last acknowledged cursor.
pub fn request_events(
    root: &Path,
    workspace_id: &str,
    after: u64,
    limit: usize,
) -> Result<Vec<Event>, EventRequestError> {
    if limit == 0 || limit > MAX_EVENT_BATCH {
        return Err(EventRequestError::Rejected(format!(
            "event batch limit must be between 1 and {MAX_EVENT_BATCH}"
        )));
    }
    let response = request_raw(
        root,
        Request {
            version: CONTRACT_VERSION,
            token: String::new(),
            command: "events".to_owned(),
            workspace_id: Some(workspace_id.to_owned()),
            after: Some(after),
            limit: Some(limit),
        },
    )?;
    serde_json::from_str(&response.state)
        .map_err(|e| EventRequestError::Rejected(format!("invalid event batch: {e}")))
}

fn request_raw(root: &Path, mut request: Request) -> Result<Response, EventRequestError> {
    let endpoint = read_endpoint(root).map_err(EventRequestError::Unavailable)?;
    request.token = endpoint.token;
    let mut stream = TcpStream::connect(("127.0.0.1", endpoint.port))
        .map_err(|e| EventRequestError::Unavailable(format!("daemon is not reachable: {e}")))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| EventRequestError::Unavailable(e.to_string()))?;
    serde_json::to_writer(&mut stream, &request)
        .map_err(|e| EventRequestError::Unavailable(e.to_string()))?;
    stream
        .write_all(b"\n")
        .map_err(|e| EventRequestError::Unavailable(e.to_string()))?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|e| EventRequestError::Unavailable(e.to_string()))?;
    let mut line = String::new();
    BufReader::new(stream)
        .take(MAX_RESPONSE_BYTES)
        .read_line(&mut line)
        .map_err(|e| EventRequestError::Unavailable(e.to_string()))?;
    let response: Response = serde_json::from_str(&line)
        .map_err(|e| EventRequestError::Rejected(format!("invalid daemon response: {e}")))?;
    if response.version != CONTRACT_VERSION || !response.ok {
        return Err(EventRequestError::Rejected(
            response
                .error
                .unwrap_or_else(|| "daemon rejected request".into()),
        ));
    }
    Ok(response)
}

/// Refuse a live instance and remove only an endpoint proven unreachable.
pub fn recover_stale_endpoint(root: &Path) -> Result<(), String> {
    if request(root, "handshake").is_ok() {
        return Err("daemon is already running".into());
    }
    let path = state_path(root);
    if !path.exists() {
        return Ok(());
    }
    let endpoint =
        read_endpoint(root).map_err(|e| format!("cannot safely recover endpoint: {e}"))?;
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], endpoint.port));
    if TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok() {
        return Err("daemon endpoint is responding but authentication failed".into());
    }
    fs::remove_file(path).map_err(|e| format!("cannot remove confirmed stale endpoint: {e}"))
}

pub fn serve(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err("selected drive is unavailable".into());
    }
    let state_dir = root.join(".aidex");
    ensure_not_symlink(&state_dir)?;
    fs::create_dir_all(&state_dir)
        .map_err(|e| format!("cannot create daemon state directory: {e}"))?;
    let dir = root.join(STATE_DIR);
    ensure_not_symlink(&dir)?;
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create daemon state directory: {e}"))?;
    let event_path = dir.join(EVENT_FILE);
    ensure_not_symlink(&event_path)?;
    let event_log = EventLog::open(&event_path, 10_000).map_err(|e| e.to_string())?;
    let endpoint_file = state_path(root);
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("cannot bind daemon endpoint: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let mut token_bytes = [0u8; 32];
    getrandom::fill(&mut token_bytes)
        .map_err(|e| format!("operating-system random source failed: {e}"))?;
    let token = token_bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let endpoint = Endpoint {
        version: CONTRACT_VERSION,
        pid: std::process::id(),
        port,
        token,
    };
    let encoded = encode_endpoint(&endpoint)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&endpoint_file).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            "daemon endpoint already exists; inspect with `aidexd status`".into()
        } else {
            format!("cannot create daemon endpoint: {e}")
        }
    })?;
    file.write_all(&encoded)
        .map_err(|e| format!("cannot publish daemon endpoint: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("cannot persist daemon endpoint: {e}"))?;
    drop(file);
    println!("ready");
    let stopping = AtomicBool::new(false);
    while !stopping.load(Ordering::Relaxed) && root.is_dir() {
        match listener.accept() {
            Ok((mut stream, _)) => handle(&mut stream, &endpoint, &stopping, &event_log),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(e) => return Err(format!("daemon listener failed: {e}")),
        }
    }
    match fs::remove_file(&endpoint_file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) if !root.is_dir() => Ok(()),
        Err(e) => Err(format!("daemon stopped but endpoint cleanup failed: {e}")),
    }
}

fn handle(
    stream: &mut TcpStream,
    endpoint: &Endpoint,
    stopping: &AtomicBool,
    event_log: &EventLog,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let input = stream.try_clone();
    let parsed = input.ok().and_then(|s| {
        let mut line = String::new();
        BufReader::new(s)
            .take(MAX_REQUEST_BYTES)
            .read_line(&mut line)
            .ok()
            .filter(|n| *n > 0)
            .and_then(|_| serde_json::from_str::<Request>(&line).ok())
    });
    let response = match parsed {
        Some(request) if request.version != CONTRACT_VERSION => {
            error("incompatible contract version")
        }
        Some(request) if request.token != endpoint.token => error("unauthorized client"),
        Some(request) if request.command == "events" => {
            match (
                request.workspace_id.as_deref(),
                request.after,
                request.limit,
            ) {
                (Some(workspace), Some(cursor), Some(limit))
                    if (1..=MAX_EVENT_BATCH).contains(&limit) =>
                {
                    match event_log.read_after(workspace, cursor, limit) {
                        Ok(events) => match serde_json::to_string(&events) {
                            Ok(body) => success(&body),
                            Err(encoding_error) => {
                                error(&format!("cannot encode event batch: {encoding_error}"))
                            }
                        },
                        Err(event_error) => error(&event_error.to_string()),
                    }
                }
                _ => error("invalid event query"),
            }
        }
        Some(request) if request.command == "handshake" => success("ready"),
        Some(request) if request.command == "status" => success("ready"),
        Some(request) if request.command == "stop" => {
            stopping.store(true, Ordering::Relaxed);
            success("stopping")
        }
        Some(_) => error("unknown command"),
        None => error("malformed request"),
    };
    if let Ok(data) = serde_json::to_vec(&response) {
        let _ = stream.write_all(&data);
        let _ = stream.write_all(b"\n");
    }
}

fn success(state: &str) -> Response {
    Response {
        version: CONTRACT_VERSION,
        ok: true,
        state: state.into(),
        error: None,
    }
}
fn error(message: &str) -> Response {
    Response {
        version: CONTRACT_VERSION,
        ok: false,
        state: "error".into(),
        error: Some(message.into()),
    }
}

fn ensure_not_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "managed daemon path is a symlink: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect managed daemon path: {error}")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpStream, sync::atomic::AtomicBool};

    #[test]
    fn unavailable_drive_is_not_recreated_by_daemon_startup() {
        let root = std::env::temp_dir().join(format!("aidexd-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        assert!(
            serve(&root)
                .unwrap_err()
                .contains("selected drive is unavailable")
        );
        assert!(!root.exists());
    }
    #[cfg(not(windows))]
    #[test]
    fn daemon_stops_after_the_selected_drive_is_removed() {
        let root = std::env::temp_dir().join(format!("aidexd-removal-{}", std::process::id()));
        let moved = root.with_extension("removed");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&moved);
        fs::create_dir_all(&root).unwrap();
        let sentinel = root.join("user-data.txt");
        fs::write(&sentinel, b"preserve").unwrap();
        let daemon_root = root.clone();
        let daemon = std::thread::spawn(move || serve(&daemon_root));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while request(&root, "status").is_err() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        request(&root, "status").unwrap();
        fs::rename(&root, &moved).unwrap();
        daemon.join().unwrap().unwrap();
        assert_eq!(fs::read(moved.join("user-data.txt")).unwrap(), b"preserve");
        fs::remove_dir_all(moved).unwrap();
    }

    #[test]
    fn endpoint_credential_is_protected_at_rest_and_round_trips() {
        let root = std::env::temp_dir().join(format!("aidexd-endpoint-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(STATE_DIR)).unwrap();
        let endpoint = Endpoint {
            version: CONTRACT_VERSION,
            pid: std::process::id(),
            port: 1,
            token: "b".repeat(64),
        };
        let bytes = encode_endpoint(&endpoint).unwrap();
        if cfg!(windows) {
            assert!(!String::from_utf8_lossy(&bytes).contains(&endpoint.token));
        }
        fs::write(state_path(&root), bytes).unwrap();
        assert_eq!(read_endpoint(&root).unwrap().token, endpoint.token);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_endpoint_recovery_removes_only_daemon_metadata() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let root =
            std::env::temp_dir().join(format!("aidexd-stale-{}-{}", std::process::id(), port));
        let runtime = root.join(STATE_DIR);
        fs::create_dir_all(&runtime).unwrap();
        let sentinel = root.join("user-data.txt");
        fs::write(&sentinel, b"preserve").unwrap();
        let endpoint = Endpoint {
            version: CONTRACT_VERSION,
            pid: 1,
            port,
            token: "a".repeat(64),
        };
        fs::write(state_path(&root), encode_endpoint(&endpoint).unwrap()).unwrap();
        recover_stale_endpoint(&root).unwrap();
        assert!(!state_path(&root).exists());
        assert_eq!(fs::read(sentinel).unwrap(), b"preserve");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn protocol_rejects_unauthorized_malformed_and_incompatible_requests() {
        let directory = tempfile::tempdir().unwrap();
        let event_log = EventLog::open(directory.path().join("events.sqlite"), 20).unwrap();
        let endpoint = Endpoint {
            version: CONTRACT_VERSION,
            pid: 1,
            port: 1,
            token: "a".repeat(64),
        };
        for (json, expected) in [
            (
                r#"{"version":1,"token":"wrong","command":"status"}"#,
                "unauthorized client",
            ),
            (
                r#"{"version":2,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","command":"status"}"#,
                "incompatible contract version",
            ),
            (
                r#"{"version":1,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","command":"status","extra":true}"#,
                "malformed request",
            ),
        ] {
            let (mut client, mut server) = tcp_pair();
            client.write_all(json.as_bytes()).unwrap();
            client.write_all(b"\n").unwrap();
            client.shutdown(Shutdown::Write).unwrap();
            handle(&mut server, &endpoint, &AtomicBool::new(false), &event_log);
            let mut response_line = String::new();
            BufReader::new(client)
                .read_line(&mut response_line)
                .unwrap();
            let response: Response = serde_json::from_str(&response_line).unwrap();
            assert!(!response.ok);
            assert_eq!(response.error.as_deref(), Some(expected));
        }
        assert!(TcpStream::connect(("127.0.0.1", 0)).is_err());
    }

    #[test]
    fn expired_event_cursor_is_a_permanent_replay_error() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let state_dir = root.join(STATE_DIR);
        fs::create_dir_all(&state_dir).unwrap();
        let event_log = EventLog::open(state_dir.join(EVENT_FILE), 1).unwrap();
        for task_id in ["first", "second"] {
            event_log
                .publish(
                    "workspace",
                    None,
                    aidex_events::EventKind::TaskStarted {
                        task_id: task_id.into(),
                    },
                )
                .unwrap();
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = Endpoint {
            version: CONTRACT_VERSION,
            pid: std::process::id(),
            port: listener.local_addr().unwrap().port(),
            token: "c".repeat(64),
        };
        fs::write(state_path(root), encode_endpoint(&endpoint).unwrap()).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            handle(&mut stream, &endpoint, &AtomicBool::new(false), &event_log);
        });

        let error = request_events(root, "workspace", 0, 1).unwrap_err();
        assert!(matches!(
            error,
            EventRequestError::Rejected(message) if message.contains("expired")
        ));
        server.join().unwrap();
    }

    #[test]
    fn authenticated_ipc_replays_from_last_acknowledged_cursor() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join(STATE_DIR)).unwrap();
        {
            let log = EventLog::open(root.join(STATE_DIR).join(EVENT_FILE), 20).unwrap();
            log.publish(
                "workspace",
                Some("session".into()),
                aidex_events::EventKind::SessionStarted,
            )
            .unwrap();
            log.publish(
                "workspace",
                Some("session".into()),
                aidex_events::EventKind::SessionFinished,
            )
            .unwrap();
        }
        let daemon_root = root.to_path_buf();
        let daemon = std::thread::spawn(move || serve(&daemon_root));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let first_batch = loop {
            match request_events(root, "workspace", 0, 1) {
                Ok(events) => break events,
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("daemon did not serve event replay: {error}"),
            }
        };
        assert_eq!(first_batch.len(), 1);
        assert_eq!(first_batch[0].sequence, 1);
        let next = request_events(root, "workspace", first_batch[0].sequence, 1).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].sequence, 2);
        assert!(
            request_events(root, "workspace", next[0].sequence, 1)
                .unwrap()
                .is_empty()
        );
        request(root, "stop").unwrap();
        daemon.join().unwrap().unwrap();

        let daemon_root = root.to_path_buf();
        let restarted_daemon = std::thread::spawn(move || serve(&daemon_root));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let replay = loop {
            match request_events(root, "workspace", 0, 2) {
                Ok(events) => break events,
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("restarted daemon did not replay events: {error}"),
            }
        };
        assert_eq!(
            replay
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        request(root, "stop").unwrap();
        restarted_daemon.join().unwrap().unwrap();
    }

    fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }
}
