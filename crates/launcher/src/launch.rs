//! Verify the active bundle, then open its desktop or run an explicit CLI command.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use aidex_bundle::{
    ActiveBundle, FileRole, SignaturePolicy, SignatureState, Target, TrustStore, UpdateJournal,
    VerifyScope, journal_path, resolve_active,
};
use aidex_drive::DrivePaths;

use crate::{
    error::LaunchError,
    locate::{current_exe, display_path, drive_for},
};

/// What the command line asks for. Only the first argument is inspected, and only
/// `--launcher-*` is reserved; everything else goes to `aidex` untouched.
pub enum Mode {
    /// Run `aidex` with these arguments.
    Run(Vec<OsString>),
    /// Verify and print the bundle report as JSON without running anything.
    Info(VerifyScope),
}

impl Mode {
    pub fn parse(args: Vec<OsString>) -> Result<Self, LaunchError> {
        let Some(first) = args.first().and_then(|arg| arg.to_str()) else {
            return Ok(Self::Run(args));
        };
        let scope = match first {
            "--launcher-info" => VerifyScope::Critical,
            "--launcher-verify" => VerifyScope::All,
            flag if flag.starts_with("--launcher-") => {
                return Err(LaunchError::usage(format!(
                    "unknown launcher option `{flag}`"
                )));
            }
            _ => return Ok(Self::Run(args)),
        };
        if args.len() > 1 {
            return Err(LaunchError::usage(format!("`{first}` takes no arguments")));
        }
        Ok(Self::Info(scope))
    }
}

/// Host-side opt-in to run a bundle that carries no `bundle.sig` (development only).
/// It never accepts a bundle whose signature is present but invalid or untrusted.
pub const ALLOW_UNSIGNED_ENV: &str = "AIDEX_ALLOW_UNSIGNED";

/// The verified state a launch is based on.
pub struct Resolved {
    pub paths: DrivePaths,
    pub active: ActiveBundle,
    /// State of an update that did not finish (recovered when the daemon starts).
    pub pending_update: Option<String>,
}

impl Resolved {
    pub fn drive_root(&self) -> PathBuf {
        display_path(self.paths.root())
    }

    pub fn bundle_dir(&self) -> PathBuf {
        display_path(&self.active.bundle.dir)
    }
}

/// Locate the drive from the launcher's location and verify the active bundle for
/// this host with `scope` — including its signature against the keys compiled into
/// this launcher and the drive's revocation list — before anything from it runs.
pub fn resolve(scope: VerifyScope) -> Result<Resolved, LaunchError> {
    let target = Target::host().ok_or(aidex_bundle::BundleError::UnsupportedHost)?;
    let exe = current_exe()?;
    let paths = drive_for(&exe, target)?;
    let trust = TrustStore::builtin()
        .and_then(|trust| trust.with_drive_revocations(&paths))
        .map_err(LaunchError::trust)?;
    let policy = SignaturePolicy {
        trust,
        allow_unsigned: env::var(ALLOW_UNSIGNED_ENV).is_ok_and(|value| value == "1"),
    };
    let active = resolve_active(&paths, target, scope, &policy)?;
    if active.signature == SignatureState::UnsignedAllowed {
        eprintln!(
            "AiDEX launcher: warning: running an UNSIGNED bundle because {ALLOW_UNSIGNED_ENV}=1; its origin is not verified (development only)"
        );
    }
    let pending_update = pending_update(&paths);
    if let Some(state) = &pending_update {
        eprintln!(
            "AiDEX launcher: note: an update was interrupted ({state}); running the version app/current.json selects, and the daemon recovers the rest when it starts"
        );
    }
    Ok(Resolved {
        paths,
        active,
        pending_update,
    })
}

/// The state of an unfinished update journal, if any. The launcher never writes the
/// journal (recovery is the daemon's job); the pointer it just verified is always a
/// complete old or new version, because it is only ever replaced by an atomic rename.
fn pending_update(paths: &DrivePaths) -> Option<String> {
    match UpdateJournal::load(&journal_path(paths)) {
        Ok(Some(journal)) if !journal.state.is_terminal() => {
            Some(journal.state.as_str().to_owned())
        }
        Ok(_) => None,
        Err(_) => Some("unreadable update journal".to_owned()),
    }
}

/// Verify the critical files, then run the CLI. On Unix the launcher process is
/// replaced (`exec`); on Windows the CLI's exit code is returned.
pub fn run(args: Vec<OsString>) -> Result<i32, LaunchError> {
    // Desktop libraries outside bin/ must also be checked before opening the app.
    let resolved = resolve(if args.is_empty() {
        VerifyScope::All
    } else {
        VerifyScope::Critical
    })?;
    let manifest = &resolved.active.bundle.manifest;
    let cli_entry = manifest
        .file(FileRole::Cli)
        .ok_or_else(|| LaunchError::usage("the bundle lists no CLI"))?;
    let cli = display_path(&resolved.active.file_path(&cli_entry.path));
    let bin = cli
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| resolved.bundle_dir());
    let desktop_name = manifest.target.executable_name("aidex-desktop");
    let desktop = manifest.files.iter().find(|entry| {
        entry.role == FileRole::Desktop
            && entry.executable
            && Path::new(&entry.path)
                .file_name()
                .is_some_and(|name| name == desktop_name.as_str())
    });
    let program = if args.is_empty() {
        desktop
            .map(|entry| display_path(&resolved.active.file_path(&entry.path)))
            .unwrap_or_else(|| cli.clone())
    } else {
        cli.clone()
    };
    let mut command = Command::new(&program);
    command
        .args(args)
        .env("AIDEX_DRIVE_ROOT", resolved.drive_root())
        .env("AIDEX_BUNDLE_DIR", resolved.bundle_dir())
        .env("AIDEX_BUNDLE_VERSION", &manifest.version)
        .env("AIDEX_BUNDLE_TARGET", manifest.target.as_str())
        .env("AIDEX_LAUNCHER_VERSION", env!("CARGO_PKG_VERSION"))
        .env("PATH", prepend_path(&bin)?);
    exec(command, &program)
}

/// `bin` followed by the inherited `PATH` entries.
fn prepend_path(bin: &Path) -> Result<OsString, LaunchError> {
    let inherited = env::var_os("PATH").unwrap_or_default();
    let entries = std::iter::once(bin.to_path_buf()).chain(env::split_paths(&inherited));
    env::join_paths(entries).map_err(|error| {
        LaunchError::io(
            format_args!("cannot add `{}` to PATH", bin.display()),
            std::io::Error::other(error.to_string()),
        )
    })
}

#[cfg(unix)]
fn exec(mut command: Command, program: &Path) -> Result<i32, LaunchError> {
    use std::os::unix::process::CommandExt;
    // `exec` only returns on failure.
    Err(LaunchError::spawn(program, command.exec()))
}

#[cfg(not(unix))]
fn exec(mut command: Command, program: &Path) -> Result<i32, LaunchError> {
    // The console delivers Ctrl+C to the CLI as well; keep waiting for it to finish
    // instead of exiting underneath it.
    let _ = ctrlc::set_handler(|| {});
    let status = command
        .status()
        .map_err(|error| LaunchError::spawn(program, error))?;
    Ok(status.code().unwrap_or(crate::error::exit::FAILURE))
}
