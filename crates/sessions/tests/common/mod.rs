//! Shared fixtures for `aidex-sessions` integration tests.
#![allow(dead_code)]

use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use aidex_core::EntityId;
use aidex_drive::DrivePaths;
use aidex_events::{Cursor, EventError, EventKind, EventScope, EventSink, MemoryEventSink};
use aidex_sessions::{HostFingerprint, NewSession, Session, SessionStore};

pub const SALT: [u8; 32] = [42; 32];

/// The laptop the tests pretend to run on.
pub fn laptop() -> HostFingerprint {
    HostFingerprint::compute(&SALT, "windows", "x86_64", "laptop-a")
}

/// Another machine with a different platform.
pub fn workstation() -> HostFingerprint {
    HostFingerprint::compute(&SALT, "linux", "x86_64", "workstation-b")
}

pub fn id(value: &str) -> EntityId {
    EntityId::new(value).unwrap()
}

/// A temporary, initialized schema-v2 drive.
pub fn temp_drive() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    aidex_drive::init_with(
        root.path(),
        aidex_drive::InitOptions {
            require_removable: false,
        },
    )
    .unwrap();
    root
}

pub fn paths(root: &Path) -> DrivePaths {
    DrivePaths::open(root).unwrap()
}

/// Open a store on `root` as `host`, recording events in memory.
pub fn open(root: &Path, host: HostFingerprint) -> (SessionStore, Arc<MemoryEventSink>) {
    let sink = Arc::new(MemoryEventSink::new());
    let store = SessionStore::open_with_host(&paths(root), sink.clone(), host).unwrap();
    (store, sink)
}

pub fn open_with_sink(
    root: &Path,
    host: HostFingerprint,
    sink: Arc<dyn EventSink>,
) -> SessionStore {
    SessionStore::open_with_host(&paths(root), sink, host).unwrap()
}

pub fn new_session(store: &SessionStore, title: &str) -> Session {
    store
        .create(NewSession::new(id("wsp_main"), id("agent_native"), title))
        .unwrap()
}

/// Names of the events recorded for `session`, in order.
pub fn session_events(sink: &MemoryEventSink, session: &EntityId) -> Vec<&'static str> {
    sink.events()
        .into_iter()
        .filter(|event| event.scope.session_id.as_ref() == Some(session))
        .map(|event| event.name())
        .collect()
}

/// Recursively copy a drive directory (the store must be closed).
pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A sink whose appends fail (a full or removed drive).
#[derive(Default)]
pub struct FailingSink;

impl EventSink for FailingSink {
    fn emit(&self, _scope: EventScope, _event: EventKind) -> Result<Cursor, EventError> {
        Err(EventError::Storage("disk I/O error".into()))
    }
}

/// A sink that kills the "process" (panics) on the first append after `arm()`,
/// simulating a crash between committing a change and appending its event.
#[derive(Default)]
pub struct CrashingSink {
    armed: AtomicBool,
    inner: MemoryEventSink,
}

impl CrashingSink {
    pub fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.inner.names()
    }
}

impl EventSink for CrashingSink {
    fn emit(&self, scope: EventScope, event: EventKind) -> Result<Cursor, EventError> {
        if self.armed.swap(false, Ordering::SeqCst) {
            panic!("simulated crash during event append");
        }
        self.inner.emit(scope, event)
    }
}

/// Every file below `dir` concatenated (to scan persisted bytes for secrets).
pub fn all_bytes(dir: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            bytes.extend(all_bytes(&entry.path()));
        } else {
            bytes.extend(fs::read(entry.path()).unwrap());
        }
    }
    bytes
}

pub fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}
