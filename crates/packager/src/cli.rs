//! Command-line definitions for `aidex-packager`.

use std::path::PathBuf;

use aidex_bundle::Target;
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "aidex-packager",
    version,
    about = "Build, sign, verify, install, and smoke-test portable AiDEX bundles"
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
    /// Generate a development or CI signing key pair (never a release key).
    Keygen(KeygenArgs),
    /// Sign a bundle directory or archive with a signing key file (writes bundle.sig).
    Sign(SignArgs),
    /// Verify a bundle directory or archive: SHA256SUMS, every file, and the signature.
    Verify(VerifyArgs),
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
    /// Public key file (`<name>.pub.json`) the built binaries trust in addition to the
    /// release keys (CI's ephemeral key or a developer key). Repeatable.
    #[arg(long, value_name = "PUBLIC_KEY_FILE")]
    pub trust_key: Vec<PathBuf>,
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
    #[command(flatten)]
    pub trust: TrustArgs,
}

/// Which signatures `install`, `smoke`, and `verify` accept.
#[derive(Debug, Args)]
pub struct TrustArgs {
    /// Also trust this public key file (`<name>.pub.json`). Repeatable.
    #[arg(long, value_name = "PUBLIC_KEY_FILE")]
    pub trust_key: Vec<PathBuf>,
    /// Accept a bundle without bundle.sig (development only; an invalid or untrusted
    /// signature is still refused).
    #[arg(long)]
    pub allow_unsigned: bool,
}

#[derive(Debug, Args)]
pub struct KeygenArgs {
    /// Key name (`[A-Za-z0-9._-]`, at most 64 characters).
    #[arg(long, default_value = "dev")]
    pub name: String,
    /// Directory for `<name>.key` (secret) and `<name>.pub.json`; keep it outside the
    /// repository.
    #[arg(long, value_name = "DIR")]
    pub out: PathBuf,
}

#[derive(Debug, Args)]
pub struct SignArgs {
    /// Secret key file written by `keygen`.
    #[arg(long, value_name = "KEY_FILE")]
    pub key: PathBuf,
    /// Bundle directory or archive to sign.
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    /// Bundle directory or archive to verify.
    #[arg(value_name = "BUNDLE")]
    pub bundle: PathBuf,
    /// Also trust this public key file (`<name>.pub.json`). Repeatable.
    #[arg(long, value_name = "PUBLIC_KEY_FILE")]
    pub trust_key: Vec<PathBuf>,
    /// Accept a bundle without bundle.sig (development only).
    #[arg(long)]
    pub allow_unsigned: bool,
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
    #[command(flatten)]
    pub trust: TrustArgs,
}

fn parse_target(value: &str) -> Result<Target, String> {
    value
        .parse()
        .map_err(|error: aidex_bundle::UnknownTarget| error.to_string())
}
