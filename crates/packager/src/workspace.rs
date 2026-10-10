//! Facts about the source tree: `cargo metadata`, git state, and the compiler.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;

use crate::error::PackagerError;

/// The subset of `cargo metadata --format-version 1` the packager uses.
#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub packages: Vec<Package>,
    pub resolve: Option<Resolve>,
    pub workspace_members: Vec<String>,
    pub workspace_root: PathBuf,
    pub target_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub id: String,
    pub license: Option<String>,
    pub license_file: Option<String>,
    pub source: Option<String>,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct Resolve {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub deps: Vec<NodeDep>,
}

#[derive(Debug, Deserialize)]
pub struct NodeDep {
    pub pkg: String,
    #[serde(default)]
    pub dep_kinds: Vec<DepKind>,
}

#[derive(Debug, Deserialize)]
pub struct DepKind {
    /// `null` for normal dependencies, `"dev"` or `"build"` otherwise.
    pub kind: Option<String>,
}

impl Metadata {
    /// Run `cargo metadata --locked` with the dependency graph filtered to `triple`.
    pub fn load(
        cargo: &OsString,
        manifest_path: Option<&Path>,
        triple: &str,
    ) -> Result<Self, PackagerError> {
        let mut command = Command::new(cargo);
        command.args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            triple,
        ]);
        if let Some(path) = manifest_path {
            command.arg("--manifest-path").arg(path);
        }
        let stdout = run_capture(&mut command, "cargo metadata")?;
        Self::parse(&stdout)
    }

    pub fn parse(json: &[u8]) -> Result<Self, PackagerError> {
        serde_json::from_slice(json)
            .map_err(|error| PackagerError::new(format!("cannot parse cargo metadata: {error}")))
    }

    /// The workspace member named `name`.
    pub fn member(&self, name: &str) -> Result<&Package, PackagerError> {
        self.packages
            .iter()
            .find(|package| package.name == name && self.workspace_members.contains(&package.id))
            .ok_or_else(|| PackagerError::new(format!("workspace package `{name}` not found")))
    }

    pub fn is_member(&self, package: &Package) -> bool {
        self.workspace_members.contains(&package.id)
    }
}

/// Git facts of the checkout, each `None` when git or the repository is unavailable.
#[derive(Clone, Debug, Default)]
pub struct GitInfo {
    pub commit: Option<String>,
    pub dirty: Option<bool>,
    /// Committer time of `HEAD` in Unix seconds.
    pub commit_time: Option<i64>,
}

impl GitInfo {
    pub fn read(workspace: &Path) -> Self {
        let git = |args: &[&str]| {
            let mut command = Command::new("git");
            command.arg("-C").arg(workspace).args(args);
            run_capture(&mut command, "git")
                .ok()
                .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
        };
        let commit = git(&["rev-parse", "HEAD"])
            .filter(|commit| aidex_bundle::validate_git_commit(commit).is_ok());
        if commit.is_none() {
            return Self::default();
        }
        Self {
            commit,
            dirty: git(&["status", "--porcelain"]).map(|status| !status.is_empty()),
            commit_time: git(&["log", "-1", "--format=%ct", "HEAD"])
                .and_then(|time| time.parse().ok()),
        }
    }
}

/// `rustc --version` as run in the workspace (so `rust-toolchain.toml` applies).
pub fn rustc_version(workspace: &Path) -> Option<String> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let mut command = Command::new(rustc);
    command.arg("--version").current_dir(workspace);
    run_capture(&mut command, "rustc")
        .ok()
        .map(|out| String::from_utf8_lossy(&out).trim().to_owned())
        .filter(|version| !version.is_empty())
}

/// Run a tool, returning stdout; stderr is passed through to the user.
pub fn run_capture(command: &mut Command, what: &str) -> Result<Vec<u8>, PackagerError> {
    let output = command
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|error| PackagerError::io(format_args!("cannot run {what}"), error))?;
    if !output.status.success() {
        return Err(PackagerError::new(format!(
            "{what} failed ({})",
            output.status
        )));
    }
    Ok(output.stdout)
}
