//! Shared fixtures for aidex-workspace integration tests.
#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use aidex_core::WorkspacePath;
use aidex_drive::DrivePaths;
use aidex_workspace::WorkspaceRegistry;

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

    pub fn registry(&self) -> WorkspaceRegistry {
        WorkspaceRegistry::open(&self.paths()).expect("open registry")
    }
}

/// A host project directory (outside the drive) with the given files.
pub fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("project dir");
    for (path, content) in files {
        write(dir.path(), path, content);
    }
    dir
}

pub fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

pub fn wp(path: &str) -> WorkspacePath {
    WorkspacePath::new(path).expect("workspace path")
}

/// Create a directory link at `link` pointing to `target`: a symbolic link on Unix,
/// a junction on Windows (junctions need no special privilege).
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

/// Create a file symbolic link; on Windows this needs Developer Mode or elevation,
/// so `false` (with a note) means the caller should skip.
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

/// Paths of every file beneath `root`, relative and `/`-separated, sorted.
pub fn files_under(root: &Path) -> Vec<String> {
    fn visit(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let file_type = entry.file_type().unwrap();
            if file_type.is_dir() && !file_type.is_symlink() {
                visit(&entry.path(), &relative, out);
            } else {
                out.push(relative);
            }
        }
    }
    let mut out = Vec::new();
    visit(root, "", &mut out);
    out.sort();
    out
}

pub fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}
