//! Shared fixtures for aidex-context integration tests.
#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use aidex_context::{ContextService, IndexOptions, IndexTarget};
use aidex_core::{CancelToken, EntityId};
use aidex_drive::DrivePaths;
use aidex_events::MemoryEventSink;
use aidex_workspace::{ConfinedRoot, WorkspaceRegistry};

/// A temporary, initialized schema-v2 drive (no removable-media check).
pub struct Drive {
    pub dir: tempfile::TempDir,
}

impl Drive {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp drive");
        aidex_drive::init_with(
            dir.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .expect("initialize drive");
        Self { dir }
    }

    pub fn paths(&self) -> DrivePaths {
        DrivePaths::open(self.dir.path()).expect("open drive paths")
    }
}

/// A host project directory with files, registered as a host workspace.
pub struct Fixture {
    pub drive: Drive,
    pub project: tempfile::TempDir,
    pub registry: WorkspaceRegistry,
    pub workspace: EntityId,
    pub events: Arc<MemoryEventSink>,
    pub service: ContextService,
}

impl Fixture {
    pub fn new(files: &[(&str, &str)]) -> Self {
        Self::with_options(files, IndexOptions::default())
    }

    pub fn with_options(files: &[(&str, &str)], options: IndexOptions) -> Self {
        let drive = Drive::new();
        let project = tempfile::tempdir().expect("project dir");
        for (path, content) in files {
            write(project.path(), path, content);
        }
        let paths = drive.paths();
        let registry = WorkspaceRegistry::open(&paths).expect("registry");
        let workspace = registry
            .add_host(project.path(), Some("project"))
            .expect("add host workspace")
            .id;
        let events = Arc::new(MemoryEventSink::new());
        let service = ContextService::open(&paths, events.clone())
            .expect("context service")
            .with_options(options);
        Self {
            drive,
            project,
            registry,
            workspace,
            events,
            service,
        }
    }

    pub fn root(&self) -> &Path {
        self.project.path()
    }

    pub fn write(&self, relative: &str, content: &str) {
        write(self.project.path(), relative, content);
    }

    pub fn target(&self) -> IndexTarget {
        let handle = self.registry.handle(&self.workspace).expect("handle");
        IndexTarget::from_handle(&handle)
    }

    pub fn confined(&self) -> ConfinedRoot {
        ConfinedRoot::new(self.project.path()).expect("confined root")
    }

    /// Build synchronously without cancellation.
    pub fn build(&self) -> aidex_context::IndexReport {
        self.service
            .build_now(&self.target(), &CancelToken::new(), &mut |_| {})
            .expect("index build")
    }

    pub fn index_dir(&self) -> PathBuf {
        self.drive.paths().index().join(self.workspace.as_str())
    }
}

pub fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
    fs::write(path, content).expect("write file");
}

/// A credential-shaped value assembled at runtime so no secret-shaped literal
/// appears in the source.
pub fn fake_token() -> String {
    format!("{}{}", "ghp_", "FAKE0fake1FAKE2fake3FAKE4fake5")
}

/// A private-key block assembled at runtime.
pub fn fake_private_key() -> String {
    let label = ["OPENSSH", "PRIVATE", "KEY"].join(" ");
    format!("-----BEGIN {label}-----\nAAAAfakefakefake\n-----END {label}-----\n")
}

/// A directory link: a symbolic link on Unix, a junction on Windows.
pub fn dir_link(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).expect("create symlink");
        true
    }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let created = status.is_ok_and(|status| status.success());
        if !created {
            eprintln!("note: could not create a junction; skipping link assertions");
        }
        created
    }
}

/// A file symbolic link; on Windows this needs Developer Mode or elevation.
pub fn file_link(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).expect("create symlink");
        true
    }
    #[cfg(windows)]
    {
        match std::os::windows::fs::symlink_file(target, link) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("note: file symlinks are not permitted here ({error}); skipping");
                false
            }
        }
    }
}
