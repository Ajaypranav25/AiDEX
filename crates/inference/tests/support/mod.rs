//! Shared fixtures: temporary drives, synthetic GGUF models, and a fake
//! llama.cpp runtime archive built from the `fake-llama-server` test binary.
#![allow(dead_code)]

use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use aidex_drive::DrivePaths;
use aidex_events::MemoryEventSink;
use aidex_host::HostResources;
use aidex_inference::{
    InferenceOptions, LocalInference,
    gguf::GgufBuilder,
    runtime::{ArchiveFormat, CatalogEntry, RuntimeCatalog, RuntimeVariant},
};
use aidex_sandbox::Sandbox;
use sha2::{Digest, Sha256};
use zip::{ZipWriter, write::SimpleFileOptions};

pub const FAKE_SERVER: &str = env!("CARGO_BIN_EXE_fake-llama-server");
pub const SANDBOX_HELPER: &str = env!("CARGO_BIN_EXE_inference-sandbox-exec");

pub fn sandbox() -> Sandbox {
    Sandbox::new(SANDBOX_HELPER)
}

/// A temporary initialized drive (no removable-media check).
pub fn temp_drive() -> (tempfile::TempDir, DrivePaths) {
    let root = tempfile::tempdir().unwrap();
    aidex_drive::init_with(
        root.path(),
        aidex_drive::InitOptions {
            require_removable: false,
        },
    )
    .unwrap();
    let paths = DrivePaths::open(root.path()).unwrap();
    (root, paths)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn sha256_file(path: &Path) -> String {
    sha256_hex(&fs::read(path).unwrap())
}

/// Write a synthetic chat model to `dir/<name>.gguf`.
pub fn write_model(dir: &Path, name: &str, builder: GgufBuilder) -> PathBuf {
    let path = dir.join(format!("{name}.gguf"));
    builder.write_to(&path).unwrap();
    path
}

/// A small chat model whose fake server behaves per the `aidex.fake.*` knobs.
pub fn fake_model(name: &str) -> GgufBuilder {
    GgufBuilder::chat_model("qwen2", name)
}

pub fn server_binary_name() -> String {
    format!("llama-server{}", std::env::consts::EXE_SUFFIX)
}

/// Zip `entries` (name, bytes, unix mode) into `path`.
pub fn write_zip(path: &Path, entries: &[(&str, &[u8], u32)]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    for (name, bytes, mode) in entries {
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(*mode);
        zip.start_file(*name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

/// A catalog entry pinning the archive at `path` for this host's platform.
pub fn catalog_entry(path: &Path, format: ArchiveFormat) -> CatalogEntry {
    let bytes = fs::read(path).unwrap();
    let platform = aidex_drive::platform_runtime_name().expect("test host platform");
    CatalogEntry {
        id: format!("fake-llama-{platform}-cpu"),
        engine: "llama.cpp".into(),
        version: "b0-test".into(),
        platform: platform.into(),
        variant: RuntimeVariant::Cpu,
        archive: path.file_name().unwrap().to_string_lossy().into_owned(),
        format,
        size_bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        url: "https://example.invalid/fake-llama.zip".into(),
        source: "https://example.invalid/fake-llama".into(),
        license: "MIT".into(),
        server_binary: server_binary_name(),
        tree_sha256: None,
        validated: false,
        notes: Some("test fixture".into()),
    }
}

/// A runtime archive holding the fake server as `llama-server` plus a library.
pub fn fake_runtime_archive(dir: &Path) -> (PathBuf, CatalogEntry) {
    let server = fs::read(FAKE_SERVER).unwrap();
    let path = dir.join("fake-llama-runtime.zip");
    let binary = server_binary_name();
    write_zip(
        &path,
        &[
            (binary.as_str(), &server, 0o755),
            ("ggml-fake.dll", b"not a real library", 0o644),
            ("LICENSE", b"MIT License (fake runtime fixture)", 0o644),
        ],
    );
    let entry = catalog_entry(&path, ArchiveFormat::Zip);
    (path, entry)
}

/// Options suited to tests: fast reaper, short health timeout, no idle unload,
/// and fixed host resources so plans do not depend on the machine's free RAM.
pub fn options() -> InferenceOptions {
    let mut options = InferenceOptions {
        idle_timeout: None,
        reaper_interval: Duration::from_millis(50),
        resources: Some(HostResources {
            total_ram_bytes: Some(16 << 30),
            available_ram_bytes: Some(8 << 30),
            physical_cores: Some(4),
            logical_cores: Some(8),
            cpu_brand: Some("test CPU".into()),
        }),
        ..InferenceOptions::default()
    };
    options.server.health_timeout = Duration::from_secs(30);
    options.server.health_interval = Duration::from_millis(25);
    options
}

/// A service over `paths` with the fake runtime's catalog.
pub fn service(
    paths: &DrivePaths,
    entry: &CatalogEntry,
    options: InferenceOptions,
) -> (LocalInference, Arc<MemoryEventSink>) {
    let events = Arc::new(MemoryEventSink::new());
    let service = LocalInference::with_catalog(
        paths,
        sandbox(),
        events.clone(),
        options,
        RuntimeCatalog::from_entries(vec![entry.clone()]).unwrap(),
    )
    .unwrap();
    (service, events)
}

/// One unauthenticated-or-not HTTP/1.1 exchange with a loopback server.
pub fn http(port: u16, method: &str, path: &str, authorization: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    if let Some(value) = authorization {
        request.push_str(&format!("Authorization: {value}\r\n"));
    }
    request.push_str("Content-Length: 0\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .unwrap_or_default();
    (status, body)
}

/// The fake server's recorded state.
pub fn fake_state(port: u16) -> serde_json::Value {
    let (status, body) = http(port, "GET", "/aidex-test/state", None);
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

pub fn wait_until(what: &str, timeout: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn port_is_closed(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(200),
    )
    .is_err()
}

/// Every file under `dir`, recursively.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

/// Whether `text` contains a run of 64 hex digits (what a leaked key looks like).
pub fn contains_hex_key(text: &str) -> bool {
    let mut run = 0;
    for ch in text.chars() {
        if ch.is_ascii_hexdigit() {
            run += 1;
            if run >= 64 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}
