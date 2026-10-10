//! Run the staged binaries before switching to them.
//!
//! [`ProcessSelfTest`] runs the binaries in the staging directory, with the drive root
//! as working directory and `AIDEX_DRIVE_ROOT` set to it: `aidex --version` and `aidexd --version` (exit 0, output names
//! the bundle version) and `aidex doctor --json` (exit 0 or 1 — doctor reports host
//! problems such as a missing optional agent with 1 — and a JSON report whose
//! `cli_version` is the bundle version). Each command has a deadline and bounded
//! output capture; a failure stops the update before anything is switched.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use aidex_bundle::{BundleManifest, FileRole, Target};
use serde::{Deserialize, Serialize};

use crate::UpdateError;

/// Most bytes kept from each output stream of a self-test command.
const MAX_CAPTURE: u64 = 256 * 1024;

/// A pre-switch check of a staged bundle.
pub trait SelfTest: Send + Sync {
    /// Check the bundle staged at `bundle_dir` (whose manifest is `manifest`) on the
    /// drive at `drive_root`.
    fn run(
        &self,
        bundle_dir: &Path,
        manifest: &BundleManifest,
        drive_root: &Path,
    ) -> Result<SelfTestReport, UpdateError>;
}

/// What a self-test ran.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelfTestReport {
    pub commands: Vec<SelfTestCommand>,
}

/// One self-test command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelfTestCommand {
    /// `aidex --version`, `aidexd --version`, `aidex doctor --json`.
    pub command: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

/// The real self-test: run the staged CLI and daemon binaries.
#[derive(Clone, Debug)]
pub struct ProcessSelfTest {
    /// Deadline for each command.
    pub timeout: Duration,
}

impl Default for ProcessSelfTest {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
        }
    }
}

/// A self-test from a closure (tests, or callers with their own checks).
pub struct SelfTestFn<F>(pub F);

impl<F> SelfTest for SelfTestFn<F>
where
    F: Fn(&Path, &BundleManifest, &Path) -> Result<SelfTestReport, UpdateError> + Send + Sync,
{
    fn run(
        &self,
        bundle_dir: &Path,
        manifest: &BundleManifest,
        drive_root: &Path,
    ) -> Result<SelfTestReport, UpdateError> {
        (self.0)(bundle_dir, manifest, drive_root)
    }
}

struct Ran {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    duration: Duration,
}

impl SelfTest for ProcessSelfTest {
    fn run(
        &self,
        bundle_dir: &Path,
        manifest: &BundleManifest,
        drive_root: &Path,
    ) -> Result<SelfTestReport, UpdateError> {
        if Target::host() != Some(manifest.target) {
            return Err(UpdateError::SelfTestFailed {
                command: "aidex --version".into(),
                detail: format!(
                    "{} binaries cannot run on this host; update a drive's {} bundle from a {} host",
                    manifest.target, manifest.target, manifest.target
                ),
            });
        }
        let binary = |role: FileRole| -> Result<PathBuf, UpdateError> {
            let file = manifest
                .file(role)
                .ok_or_else(|| UpdateError::SelfTestFailed {
                    command: role.as_str().into(),
                    detail: "the manifest lists no such binary".into(),
                })?;
            Ok(file
                .path
                .split('/')
                .fold(bundle_dir.to_path_buf(), |path, part| path.join(part)))
        };
        let cli = binary(FileRole::Cli)?;
        let daemon = binary(FileRole::Daemon)?;
        let bin = cli
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| bundle_dir.to_path_buf());
        let version = &manifest.version;
        let mut report = SelfTestReport::default();
        let mut record = |name: &str, ran: &Ran| {
            report.commands.push(SelfTestCommand {
                command: name.to_owned(),
                exit_code: ran.exit_code,
                duration_ms: u64::try_from(ran.duration.as_millis()).unwrap_or(u64::MAX),
            });
        };
        for (name, program, args) in [
            ("aidex --version", &cli, &["--version"][..]),
            ("aidexd --version", &daemon, &["--version"][..]),
        ] {
            // The drive root is the working directory: the staging directory is
            // renamed into `app/versions/` right after, which Windows refuses while
            // it is any process's working directory.
            let ran = self.spawn(program, args, &bin, drive_root, drive_root, name)?;
            record(name, &ran);
            if ran.exit_code != Some(0) || !ran.stdout.contains(version.as_str()) {
                return Err(failure(
                    name,
                    &ran,
                    &format!("expected exit 0 and version {version}"),
                ));
            }
        }
        let name = "aidex doctor --json";
        let ran = self.spawn(
            &cli,
            &["doctor", "--json"],
            &bin,
            drive_root,
            drive_root,
            name,
        )?;
        record(name, &ran);
        let report_version = serde_json::from_str::<serde_json::Value>(ran.stdout.trim())
            .ok()
            .and_then(|json| json["cli_version"].as_str().map(str::to_owned));
        if !matches!(ran.exit_code, Some(0 | 1)) || report_version.as_deref() != Some(version) {
            return Err(failure(
                name,
                &ran,
                &format!("expected exit 0 or 1 and a JSON report with cli_version {version}"),
            ));
        }
        Ok(report)
    }
}

impl ProcessSelfTest {
    fn spawn(
        &self,
        program: &Path,
        args: &[&str],
        bin: &Path,
        cwd: &Path,
        drive_root: &Path,
        name: &str,
    ) -> Result<Ran, UpdateError> {
        let started = Instant::now();
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(
            std::iter::once(plain(bin)).chain(std::env::split_paths(&inherited)),
        )
        .unwrap_or(inherited);
        let mut child = Command::new(plain(program))
            .args(args)
            .current_dir(plain(cwd))
            .env("AIDEX_DRIVE_ROOT", plain(drive_root))
            .env("PATH", path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| UpdateError::SelfTestFailed {
                command: name.to_owned(),
                detail: format!("cannot start `{}`: {error}", program.display()),
            })?;
        let drain = |pipe: Option<Box<dyn Read + Send>>| {
            thread::spawn(move || {
                let mut kept = Vec::new();
                if let Some(mut pipe) = pipe {
                    let _ = (&mut pipe).take(MAX_CAPTURE).read_to_end(&mut kept);
                    // Keep reading so the child never blocks on a full pipe.
                    let _ = std::io::copy(&mut pipe, &mut std::io::sink());
                }
                String::from_utf8_lossy(&kept).into_owned()
            })
        };
        let stdout = drain(
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );
        let stderr = drain(
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if started.elapsed() >= self.timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    let _ = child.kill();
                    return Err(UpdateError::io(format!("cannot wait for `{name}`"), error));
                }
            }
        };
        let ran = Ran {
            exit_code: status.and_then(|status| status.code()),
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
            duration: started.elapsed(),
        };
        if status.is_none() {
            return Err(failure(
                name,
                &ran,
                &format!("did not finish within {} s", self.timeout.as_secs()),
            ));
        }
        Ok(ran)
    }
}

fn failure(name: &str, ran: &Ran, expected: &str) -> UpdateError {
    let clip = |text: &str| -> String {
        let text = text.trim();
        let mut cut = text.len().min(400);
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text[..cut].to_owned()
    };
    UpdateError::SelfTestFailed {
        command: name.to_owned(),
        detail: format!(
            "{expected}; got exit {:?}, stdout `{}`, stderr `{}`",
            ran.exit_code,
            clip(&ran.stdout),
            clip(&ran.stderr)
        ),
    }
}

/// Strip the Windows verbatim prefix (`\\?\C:\x` → `C:\x`) for child processes.
pub(crate) fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}
