//! Shared helpers for aidexd integration tests.
#![allow(dead_code)]

use aidexd::{Client, ClientInfo, Daemon, DaemonHandle, DaemonOptions, Endpoint};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::Path,
    time::{Duration, Instant},
};

/// Generous bound for anything that should happen "promptly".
pub const PATIENCE: Duration = Duration::from_secs(30);

/// A temporary, initialized schema-v2 AiDEX drive (developer folder; no removable check).
pub fn temp_drive() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    init_drive(root.path());
    root
}

/// Initialize `root` as an AiDEX drive without requiring removable media.
pub fn init_drive(root: &Path) {
    aidex_drive::init_with(
        root,
        aidex_drive::InitOptions {
            require_removable: false,
        },
    )
    .expect("initialize test drive");
}

pub fn options() -> DaemonOptions {
    DaemonOptions {
        poll_interval: Duration::from_millis(10),
        ..DaemonOptions::default()
    }
}

pub fn start(
    root: &Path,
    options: DaemonOptions,
    configure: impl FnOnce(&mut Daemon),
) -> DaemonHandle {
    let mut daemon = Daemon::with_options(root, options).expect("compose daemon");
    configure(&mut daemon);
    daemon.start().expect("start daemon")
}

pub fn client(endpoint: &Endpoint) -> Client {
    Client::connect_endpoint(endpoint, ClientInfo::new("test-client", "1.0")).expect("connect")
}

pub fn handshake(endpoint: &Endpoint) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "aidex.handshake",
        "params": {
            "contract_version": 2,
            "token": endpoint.token.expose(),
            "client": { "name": "raw-test", "version": "1" }
        }
    })
}

/// A hand-driven JSON-RPC connection for wire-level assertions.
pub struct Raw {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Raw {
    pub fn connect(endpoint: &Endpoint) -> Self {
        let stream = TcpStream::connect(("127.0.0.1", endpoint.port)).expect("tcp connect");
        stream.set_read_timeout(Some(PATIENCE)).unwrap();
        Self {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
        }
    }

    /// Connect and complete a valid handshake.
    pub fn authenticated(endpoint: &Endpoint) -> Self {
        let mut raw = Self::connect(endpoint);
        raw.send(&handshake(endpoint));
        let reply = raw.recv();
        assert_eq!(reply["result"]["contract_version"], 2, "{reply}");
        raw
    }

    pub fn send(&mut self, message: &Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.send_bytes(line.as_bytes());
    }

    pub fn send_bytes(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("send");
    }

    pub fn recv(&mut self) -> Value {
        self.recv_or_eof()
            .expect("connection closed before a message arrived")
    }

    pub fn recv_or_eof(&mut self) -> Option<Value> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(serde_json::from_str(&line).expect("daemon sent invalid JSON")),
            Err(error) if is_reset(&error) => None,
            Err(error) => panic!("read failed: {error}"),
        }
    }

    /// Assert the daemon closes the connection (draining anything still queued).
    pub fn expect_closed(&mut self) {
        let deadline = Instant::now() + PATIENCE;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            assert!(Instant::now() < deadline, "connection was not closed");
            match self.reader.read(&mut buffer) {
                Ok(0) => return,
                Ok(_) => {}
                Err(error) if is_reset(&error) => return,
                Err(error) => panic!("connection was not closed: {error}"),
            }
        }
    }

    pub fn into_stream(self) -> TcpStream {
        self.writer
    }
}

fn is_reset(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
    )
}

/// A request; `null` params are omitted (JSON-RPC requires structured params).
pub fn request(id: i64, method: &str, params: Value) -> Value {
    let mut message = json!({ "jsonrpc": "2.0", "id": id, "method": method });
    if !params.is_null() {
        message["params"] = params;
    }
    message
}

/// Poll `condition` until it holds or [`PATIENCE`] elapses.
pub fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
