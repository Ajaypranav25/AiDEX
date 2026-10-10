//! `aidex-packager smoke`: install an archive into a fresh developer drive and prove
//! that the launcher, CLI, and daemon run from it without leaving host residue.
//!
//! Steps: check the archive against `SHA256SUMS` (when present) → initialize
//! `<work>/drive` (removable check skipped) → snapshot host directories → install
//! (signature checked against the built-in keys plus `--trust-key`) →
//! run the drive-root entry with `--launcher-verify`, `--version`, `doctor --json`,
//! `update verify`, `daemon start`, `daemon status --json`, `daemon stop` → compare
//! snapshots → check
//! that the drive root holds only AiDEX-owned names.

mod process;
mod residue;

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use aidex_bundle::{
    InstallReport, InstallSource, SHA256SUMS_FILE_NAME, Sha256Sums, SignatureState, Target,
    install_signed,
    layout::{PREVIOUS_SUFFIX, UNIX_ROOT_LAUNCHER, WINDOWS_ROOT_LAUNCHER},
    sha256_file,
};
use aidex_drive::{DriveLayout, InitOptions, MANIFEST_FILE_NAME, init_with};
use serde::Serialize;
use serde_json::Value;

use crate::{cli::SmokeArgs, error::PackagerError, signing::policy};
use process::Ran;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Checksum {
    /// The archive matched its `SHA256SUMS` entry.
    Verified,
    /// No `SHA256SUMS` next to the archive.
    Absent,
}

#[derive(Debug, Serialize)]
pub struct DoctorSummary {
    pub exit_code: Option<i32>,
    pub ok: Option<bool>,
    pub drive_ok: Option<bool>,
    pub drive_error: Option<String>,
    pub oh_my_pi_available: Option<bool>,
    /// The bundled CLI's own integrity report (`doctor --json` → `integrity`).
    pub integrity_verified: Option<bool>,
    pub signer_key_id: Option<String>,
    pub sbom_valid: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SmokeReport {
    pub ok: bool,
    pub archive: PathBuf,
    pub archive_sha256: String,
    pub checksum: Checksum,
    pub drive: PathBuf,
    pub install_ms: u128,
    pub install: InstallReport,
    pub entry: PathBuf,
    pub commands: Vec<Ran>,
    pub doctor: DoctorSummary,
    pub residue_watched: Vec<PathBuf>,
    pub residue_found: Vec<PathBuf>,
    pub drive_root_entries: Vec<String>,
}

pub fn run(args: &SmokeArgs) -> Result<SmokeReport, PackagerError> {
    let timeout = Duration::from_secs(args.timeout.max(1));
    let (archive_sha256, _) = sha256_file(&args.archive).map_err(|error| {
        PackagerError::io(
            format_args!("cannot read `{}`", args.archive.display()),
            error,
        )
    })?;
    let checksum = check_sums(&args.archive, &archive_sha256)?;
    let policy = policy(&args.trust.trust_key, args.trust.allow_unsigned)?;

    fs::create_dir_all(&args.work).map_err(|error| {
        PackagerError::io(
            format_args!("cannot create `{}`", args.work.display()),
            error,
        )
    })?;
    let work = std::path::absolute(&args.work).map_err(|error| {
        PackagerError::io(
            format_args!("cannot resolve `{}`", args.work.display()),
            error,
        )
    })?;
    let drive = work.join("drive");
    if fs::read_dir(&drive).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(PackagerError::new(format!(
            "`{}` is not empty; use a fresh --work directory",
            drive.display()
        )));
    }
    fs::create_dir_all(&drive).map_err(|error| {
        PackagerError::io(format_args!("cannot create `{}`", drive.display()), error)
    })?;
    init_with(
        &drive,
        InitOptions {
            require_removable: false,
        },
    )?;
    let watched = residue::watched();
    let before = residue::Snapshot::take(&watched);

    let started = Instant::now();
    let report = install_signed(
        &InstallSource::from_path(&args.archive)?,
        &drive,
        &policy,
        &mut |_| Ok(()),
    )?;
    let install_ms = started.elapsed().as_millis();
    progress(&format!(
        "installed {} into {} ({install_ms} ms)",
        report.dir,
        drive.display()
    ));
    let host =
        Target::host().ok_or_else(|| PackagerError::new("this host has no bundle target"))?;
    if report.target != host {
        return Err(PackagerError::new(format!(
            "the archive is for {}, but this host is {host}; smoke-test each target on its own OS",
            report.target
        )));
    }
    let entry = drive.join(if host.is_windows() {
        WINDOWS_ROOT_LAUNCHER
    } else {
        UNIX_ROOT_LAUNCHER
    });

    let mut commands = Vec::new();
    // Unsigned bundles run only with the launcher's explicit host-side opt-in.
    let env: Vec<(&str, &str)> = if args.trust.allow_unsigned {
        vec![("AIDEX_ALLOW_UNSIGNED", "1")]
    } else {
        Vec::new()
    };
    let mut step =
        |arguments: &[&str]| step(&entry, arguments, &work, &env, timeout, &mut commands);
    let verify = step(&["--launcher-verify"])?;
    let version = step(&["--version"])?;
    let doctor = step(&["doctor", "--json"])?;
    let integrity = step(&["--json", "update", "verify"])?;
    // Always try to stop the daemon, even when starting or querying it failed.
    let start = step(&["daemon", "start"]);
    let daemon_status = step(&["daemon", "status", "--json"]);
    let stop = step(&["daemon", "stop"]);
    let (start, daemon_status, stop) = (start?, daemon_status?, stop?);

    let expect = |index: usize, codes: &[i32]| -> Result<&Ran, PackagerError> {
        let ran = &commands[index];
        if ran.exit_code.is_some_and(|code| codes.contains(&code)) {
            Ok(ran)
        } else {
            Err(PackagerError::new(format!(
                "`{}` exited with {:?}{}",
                ran.args.join(" "),
                ran.exit_code,
                ran.stderr_suffix()
            )))
        }
    };
    let verified = expect(verify, &[0])?.json()?;
    if verified["bundle"]["version"] != report.version.as_str()
        || verified["bundle"]["scope"] != "all"
    {
        return Err(PackagerError::new(format!(
            "unexpected launcher report: {verified}"
        )));
    }
    let version_line = expect(version, &[0])?.stdout.trim().to_owned();
    if !version_line.contains(&report.version) {
        return Err(PackagerError::new(format!(
            "`--version` printed `{version_line}`, expected version {}",
            report.version
        )));
    }
    // doctor exits 1 when Oh My Pi is missing or the folder is not removable media;
    // the smoke requires a well-formed report from the bundled CLI, not a healthy host.
    let doctor_json = expect(doctor, &[0, 1])?.json()?;
    if doctor_json["cli_version"] != report.version.as_str() {
        return Err(PackagerError::new(format!(
            "unexpected doctor report: {doctor_json}"
        )));
    }
    let doctor = DoctorSummary {
        exit_code: commands[doctor].exit_code,
        ok: doctor_json["ok"].as_bool(),
        drive_ok: doctor_json["drive"]["ok"].as_bool(),
        drive_error: doctor_json["drive"]["error"].as_str().map(str::to_owned),
        oh_my_pi_available: doctor_json["oh_my_pi"]["available"].as_bool(),
        integrity_verified: doctor_json["integrity"]["verified"].as_bool(),
        signer_key_id: doctor_json["integrity"]["signer_key_id"]
            .as_str()
            .map(str::to_owned),
        sbom_valid: doctor_json["integrity"]["sbom"]["valid"].as_bool(),
    };
    // The bundled CLI must confirm what the installer verified: same signer, valid SBOM.
    let expected_signer = match &report.signature {
        Some(SignatureState::Verified { key_id, .. }) => Some(key_id.clone()),
        _ => None,
    };
    if doctor.integrity_verified != Some(true)
        || doctor.signer_key_id != expected_signer
        || doctor.sbom_valid != Some(true)
    {
        return Err(PackagerError::new(format!(
            "the bundled CLI does not report a verified installation (signer {:?}, expected {expected_signer:?}): {}",
            doctor.signer_key_id, doctor_json["integrity"]
        )));
    }
    // The bundled CLI re-verifies every file and the signature (`update verify`).
    let verified_install = expect(integrity, &[0])?.json()?;
    if verified_install["verified"] != true
        || verified_install["scope"] != "all"
        || verified_install["signer_key_id"].as_str() != expected_signer.as_deref()
    {
        return Err(PackagerError::new(format!(
            "unexpected `update verify` report: {verified_install}"
        )));
    }
    expect(start, &[0])?;
    let status_json = expect(daemon_status, &[0])?.json()?;
    check_daemon(&status_json, &drive, &report.version)?;
    expect(stop, &[0])?;

    let after = residue::Snapshot::take(&watched);
    let residue_found = before.new_entries(&after);
    if !residue_found.is_empty() {
        return Err(PackagerError::new(format!(
            "host residue outside the drive: {}",
            residue_found
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let drive_root_entries = check_drive_root(&drive)?;
    Ok(SmokeReport {
        ok: true,
        archive: args.archive.clone(),
        archive_sha256,
        checksum,
        drive,
        install_ms,
        install: report,
        entry,
        commands,
        doctor,
        residue_watched: before.dirs(),
        residue_found,
        drive_root_entries,
    })
}

fn progress(line: &str) {
    eprintln!("smoke: {line}");
}

/// Run the drive entry with `arguments`, record it, and return its index.
fn step(
    entry: &Path,
    arguments: &[&str],
    cwd: &Path,
    env: &[(&str, &str)],
    timeout: Duration,
    commands: &mut Vec<Ran>,
) -> Result<usize, PackagerError> {
    let ran = process::run(entry, arguments, cwd, env, timeout)?;
    progress(&format!(
        "{} {} -> exit {} ({} ms)",
        entry.display(),
        arguments.join(" "),
        ran.exit_code
            .map_or_else(|| "signal".into(), |code| code.to_string()),
        ran.duration_ms
    ));
    commands.push(ran);
    Ok(commands.len() - 1)
}

fn check_sums(archive: &Path, sha256: &str) -> Result<Checksum, PackagerError> {
    let sums_path = archive
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SHA256SUMS_FILE_NAME);
    if !sums_path.is_file() {
        return Ok(Checksum::Absent);
    }
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PackagerError::new("the archive name is not UTF-8"))?;
    match Sha256Sums::load(&sums_path)?.get(name) {
        Some(expected) if expected == sha256 => Ok(Checksum::Verified),
        Some(expected) => Err(PackagerError::new(format!(
            "`{name}` has sha256 {sha256}, but SHA256SUMS lists {expected}"
        ))),
        None => Err(PackagerError::new(format!(
            "SHA256SUMS does not list `{name}`"
        ))),
    }
}

fn check_daemon(status: &Value, drive: &Path, version: &str) -> Result<(), PackagerError> {
    let pid = status["pid"].as_u64().unwrap_or(0);
    let root = status["drive_root"].as_str().map(PathBuf::from);
    let same_root = root
        .as_deref()
        .and_then(|root| fs::canonicalize(root).ok())
        .zip(fs::canonicalize(drive).ok())
        .is_some_and(|(reported, expected)| reported == expected);
    if pid == 0 || !same_root || status["version"] != version {
        return Err(PackagerError::new(format!(
            "unexpected daemon status for `{}`: {status}",
            drive.display()
        )));
    }
    Ok(())
}

/// The drive root may hold only the manifest, `.aidex`, launcher entries, and the
/// top-level directories of the layout.
fn check_drive_root(drive: &Path) -> Result<Vec<String>, PackagerError> {
    let mut allowed: BTreeSet<String> = [
        MANIFEST_FILE_NAME,
        ".aidex",
        WINDOWS_ROOT_LAUNCHER,
        UNIX_ROOT_LAUNCHER,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for name in [WINDOWS_ROOT_LAUNCHER, UNIX_ROOT_LAUNCHER] {
        allowed.insert(format!("{name}{PREVIOUS_SUFFIX}"));
    }
    for (_, path) in DriveLayout::default().entries() {
        if let Some(top) = path.as_str().split('/').next() {
            allowed.insert(top.to_owned());
        }
    }
    let entries: Vec<String> = fs::read_dir(drive)
        .map_err(|error| {
            PackagerError::io(format_args!("cannot list `{}`", drive.display()), error)
        })?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let unexpected: Vec<&String> = entries
        .iter()
        .filter(|name| !allowed.contains(*name))
        .collect();
    if !unexpected.is_empty() {
        return Err(PackagerError::new(format!(
            "unexpected entries at the drive root: {unexpected:?}"
        )));
    }
    let mut entries = entries;
    entries.sort();
    Ok(entries)
}
