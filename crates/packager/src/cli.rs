//! Command-line definitions for `aidex-packager`.

use std::path::PathBuf;

use aidex_bundle::Target;
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "aidex-packager",
    version,
    about = "Build, install, and smoke-test portable AiDEX bundles"
)]
pub struct Cli {
    /// Print one JSON report on stdout instead of human-readable text.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Build release binaries for a target and package them as a verified bundle,
    /// a deterministic archive, and a SHA256SUMS entry.
    Build(BuildArgs),
    /// Install a bundle (archive or directory) onto an initialized AiDEX drive.
    Install(InstallArgs),
    /// Install an archive into a fresh developer drive and exercise the launcher,
    /// CLI, and daemon from it, checking for host residue.
    Smoke(SmokeArgs),
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    /// Bundle target (windows-x64, linux-x64, linux-arm64, macos-arm64, macos-x64).
    #[arg(long, value_parser = parse_target)]
    pub target: Target,
    /// Output directory for the bundle directory, archive, and SHA256SUMS.
    #[arg(long, default_value = "dist")]
    pub out: PathBuf,
    /// Rust target triple (defaults to the target's static triple, e.g. musl on Linux).
    #[arg(long)]
    pub triple: Option<String>,
    /// Cargo target directory for the release build (default `<target>/packager`).
    #[arg(long)]
    pub target_dir: Option<PathBuf>,
    /// Workspace `Cargo.toml` (default: the workspace containing the current directory).
    #[arg(long)]
    pub manifest_path: Option<PathBuf>,
    /// Prebuilt desktop app (file or directory) to include under `desktop/`.
    #[arg(long, value_name = "PATH")]
    pub desktop: Option<PathBuf>,
    /// Prebuilt installer app (file or directory) to include under `installer/`.
    #[arg(long, value_name = "PATH")]
    pub installer: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Bundle archive (.zip / .tar.gz) or unpacked bundle directory.
    #[arg(long, value_name = "ARCHIVE_OR_DIR")]
    pub bundle: PathBuf,
    /// Root of an initialized AiDEX drive (`aidex init --drive <root>`).
    #[arg(long, value_name = "DRIVE_PATH")]
    pub drive: PathBuf,
    /// First initialize DRIVE_PATH as a developer drive folder, skipping the
    /// removable-media check (for tests and CI; never use on a real drive by accident).
    #[arg(long)]
    pub init_dev_drive: bool,
}

#[derive(Debug, Args)]
pub struct SmokeArgs {
    /// Bundle archive built for this host.
    #[arg(long)]
    pub archive: PathBuf,
    /// Empty or missing work directory; the developer drive is created inside it.
    #[arg(long)]
    pub work: PathBuf,
    /// Seconds allowed for each launched command.
    #[arg(long, default_value_t = 120)]
    pub timeout: u64,
}

fn parse_target(value: &str) -> Result<Target, String> {
    value
        .parse()
        .map_err(|error: aidex_bundle::UnknownTarget| error.to_string())
}
