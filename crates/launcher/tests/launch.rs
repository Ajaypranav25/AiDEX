//! End-to-end launcher behaviour against a temporary drive with an installed bundle
//! whose `aidex` is a test stand-in that reports its arguments and environment.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use aidex_bundle::{
    BundleFile, BundleManifest, CurrentPointer, DEV_TRUSTED_KEYS_ENV, FileRole, InstallSource,
    JournalOperation, JournalState, KeyPair, LAUNCHER_MARKER, MANIFEST_FILE_NAME,
    MANIFEST_SCHEMA_VERSION, PRODUCT_NAME, Revocation, RevocationList, StatePaths, Target,
    UPDATE_ID_PREFIX, UpdateJournal, install, journal_path, layout, read_manifest_bytes,
    revocations_path, sign_manifest, write_signature,
};
use serde_json::Value;

const LAUNCHER: &str = env!("CARGO_BIN_EXE_aidex-launcher");
const TEST_CLI: &str = env!("CARGO_BIN_EXE_aidex-launcher-test-cli");

fn host() -> Target {
    Target::host().expect("tests run on a named bundle target")
}

struct Drive {
    root: tempfile::TempDir,
    target: Target,
    bundle_dir: PathBuf,
    /// The key the installed bundle is signed with; trusted through
    /// `AIDEX_DEV_TRUSTED_KEYS` (honoured because test builds enable `dev-trust`).
    key: KeyPair,
}

fn sign(dir: &Path, key: &KeyPair) {
    let bytes = read_manifest_bytes(dir).unwrap();
    write_signature(dir, &sign_manifest(&bytes, key)).unwrap();
}

fn trusted_keys_env(key: &KeyPair) -> String {
    format!("launcher-tests={}", key.public_key_hex())
}

/// Write a bundle for `target` whose CLI is the test stand-in (real launcher binary,
/// placeholder daemon/helper files) into `dir`.
fn write_bundle(dir: &Path, target: Target, version: &str) {
    fs::create_dir_all(dir.join("bin")).unwrap();
    let mut files = Vec::new();
    let launcher_bin = layout::bin_path(layout::LAUNCHER_BINARY, target);
    let cli_bin = layout::bin_path(layout::CLI_BINARY, target);
    if target == host() {
        fs::copy(LAUNCHER, dir.join(&launcher_bin)).unwrap();
        fs::copy(TEST_CLI, dir.join(&cli_bin)).unwrap();
    } else {
        fs::write(
            dir.join(&launcher_bin),
            format!("foreign launcher\n{LAUNCHER_MARKER}\n"),
        )
        .unwrap();
        fs::write(dir.join(&cli_bin), "foreign cli").unwrap();
    }
    files.push(BundleFile::describe(dir, &launcher_bin, FileRole::Launcher, true).unwrap());
    files.push(BundleFile::describe(dir, &cli_bin, FileRole::Cli, true).unwrap());
    for (role, stem) in [
        (FileRole::Daemon, layout::DAEMON_BINARY),
        (FileRole::Helper, layout::HELPER_BINARY),
    ] {
        let path = layout::bin_path(stem, target);
        fs::write(dir.join(&path), format!("placeholder {stem}")).unwrap();
        files.push(BundleFile::describe(dir, &path, role, true).unwrap());
    }
    let mut manifest = BundleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        product: PRODUCT_NAME.into(),
        version: version.into(),
        git_commit: None,
        source_dirty: None,
        target,
        rust_target: target.default_rust_triple().into(),
        rustc: None,
        created_at: aidex_core::Timestamp::from_unix_millis(1_760_000_000_000).unwrap(),
        min_drive_schema: aidex_drive::CURRENT_SCHEMA_VERSION,
        state: StatePaths::current(),
        files,
        licenses: Vec::new(),
    };
    manifest.normalize();
    fs::write(dir.join(MANIFEST_FILE_NAME), manifest.to_json().unwrap()).unwrap();
}

impl Drive {
    fn new() -> Self {
        Self::with_bundle(true)
    }

    fn with_bundle(signed: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        aidex_drive::init_with(
            root.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .unwrap();
        let target = host();
        let source = tempfile::tempdir().unwrap();
        let dir = source.path().join("bundle");
        write_bundle(&dir, target, "0.1.0");
        let key = KeyPair::generate("launcher-tests").unwrap();
        if signed {
            sign(&dir, &key);
        }
        let report = install(&InstallSource::Directory(dir), root.path()).unwrap();
        let bundle_dir = root.path().join("app").join(&report.dir);
        Self {
            root,
            target,
            bundle_dir,
            key,
        }
    }

    fn paths(&self) -> aidex_drive::DrivePaths {
        aidex_drive::DrivePaths::open(self.path()).unwrap()
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    /// The launcher as users start it: `AiDEX.exe` or `app/launcher/<t>/aidex-launcher`.
    fn launcher(&self) -> PathBuf {
        let paths = aidex_drive::DrivePaths::open(self.path()).unwrap();
        layout::launcher_path(&paths, self.target)
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[])
    }

    /// Run the launcher trusting the drive's signing key, plus `env` overrides (an
    /// empty value removes a variable).
    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let cwd = tempfile::tempdir().unwrap();
        let mut command = Command::new(self.launcher());
        command
            .args(args)
            .current_dir(cwd.path())
            .env("AIDEX_DRIVE_ROOT", "/caller/supplied/value/is/ignored")
            .env(DEV_TRUSTED_KEYS_ENV, trusted_keys_env(&self.key))
            .env_remove("AIDEX_ALLOW_UNSIGNED");
        for (name, value) in env {
            if value.is_empty() {
                command.env_remove(name);
            } else {
                command.env(name, value);
            }
        }
        command.output().unwrap()
    }

    fn pointer(&self) -> PathBuf {
        self.path().join("app").join("current.json")
    }

    fn edit_pointer(&self, edit: impl FnOnce(&mut Value)) {
        let mut value: Value = serde_json::from_slice(&fs::read(self.pointer()).unwrap()).unwrap();
        edit(&mut value["targets"][self.target.as_str()]);
        fs::write(self.pointer(), value.to_string()).unwrap();
    }
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON ({error}): {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn same_dir(reported: &Value, expected: &Path) {
    let reported = PathBuf::from(reported.as_str().expect("path is a string"));
    assert_eq!(
        fs::canonicalize(&reported).unwrap(),
        fs::canonicalize(expected).unwrap(),
        "{reported:?}"
    );
}

#[test]
fn the_launcher_runs_the_cli_with_the_drive_root_and_passthrough_arguments() {
    let drive = Drive::new();
    let output = drive.run(&["exit", "7", "--json", "two words", "--launcher-info"]);
    assert_eq!(output.status.code(), Some(7), "{}", stderr(&output));
    let report = stdout_json(&output);
    assert_eq!(
        report["args"],
        serde_json::json!(["exit", "7", "--json", "two words", "--launcher-info"])
    );
    same_dir(&report["drive_root"], drive.path());
    same_dir(&report["bundle_dir"], &drive.bundle_dir);
    same_dir(&report["path_first"], &drive.bundle_dir.join("bin"));
    assert_eq!(report["bundle_version"], "0.1.0");
    assert_eq!(report["bundle_target"], drive.target.as_str());
    assert_eq!(report["launcher_version"], env!("CARGO_PKG_VERSION"));

    let output = drive.run(&[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout_json(&output)["args"], serde_json::json!([]));
}

#[cfg(unix)]
#[test]
fn the_unix_entry_script_selects_the_host_launcher() {
    let drive = Drive::new();
    let output = Command::new("sh")
        .arg(drive.path().join("aidex"))
        .args(["exit", "3"])
        .env(DEV_TRUSTED_KEYS_ENV, trusted_keys_env(&drive.key))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    same_dir(&stdout_json(&output)["drive_root"], drive.path());
}

#[test]
fn launching_writes_nothing_to_the_drive() {
    let drive = Drive::new();
    let before = snapshot(drive.path());
    let output = drive.run(&["--version"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(snapshot(drive.path()), before);
}

#[test]
fn a_tampered_binary_is_refused_before_anything_runs() {
    let drive = Drive::new();
    let daemon = drive
        .bundle_dir
        .join(layout::bin_path(layout::DAEMON_BINARY, drive.target));
    fs::write(&daemon, "tampered daemon").unwrap();
    let output = drive.run(&["exit", "0"]);
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "the CLI must not run");
    let message = stderr(&output);
    assert!(message.contains("integrity check failed"), "{message}");
    assert!(message.contains("aidexd"), "{message}");
    assert!(message.contains("nothing was run"), "{message}");
}

#[test]
fn a_tampered_cli_is_refused() {
    let drive = Drive::new();
    let cli = drive
        .bundle_dir
        .join(layout::bin_path(layout::CLI_BINARY, drive.target));
    let mut bytes = fs::read(&cli).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&cli, bytes).unwrap();
    let output = drive.run(&["exit", "0"]);
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
}

#[test]
fn a_planted_file_beside_the_binaries_is_refused() {
    let drive = Drive::new();
    fs::write(drive.bundle_dir.join("bin").join("version.dll"), "planted").unwrap();
    let output = drive.run(&[]);
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("not listed"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_missing_pointer_is_reported_as_not_installed() {
    let drive = Drive::new();
    fs::remove_file(drive.pointer()).unwrap();
    let output = drive.run(&[]);
    assert_eq!(output.status.code(), Some(8), "{}", stderr(&output));
    assert!(stderr(&output).contains("no AiDEX bundle is installed"));
    assert!(stderr(&output).contains("aidex-packager install"));
}

#[test]
fn a_pointer_escaping_app_versions_is_refused() {
    let drive = Drive::new();
    for dir in [
        "../../outside",
        "versions/../../outside",
        "versions/a/b",
        "/abs/path",
    ] {
        drive.edit_pointer(|entry| entry["dir"] = dir.into());
        let output = drive.run(&[]);
        assert_eq!(output.status.code(), Some(11), "{dir}: {}", stderr(&output));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn a_bundle_for_another_target_is_refused() {
    let drive = Drive::new();
    let other = if drive.target == Target::LinuxX64 {
        Target::WindowsX64
    } else {
        Target::LinuxX64
    };
    let source = tempfile::tempdir().unwrap();
    write_bundle(source.path(), other, "0.1.0");
    sign(source.path(), &drive.key);
    let report = install(
        &InstallSource::Directory(source.path().to_path_buf()),
        drive.path(),
    )
    .unwrap();
    drive.edit_pointer(|entry| {
        entry["dir"] = report.dir.clone().into();
        entry["manifest_sha256"] = report.manifest_sha256.clone().into();
    });
    let output = drive.run(&[]);
    assert_eq!(output.status.code(), Some(10), "{}", stderr(&output));
    assert!(
        stderr(&output).contains(&format!("built for {other}")),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_launcher_outside_an_aidex_drive_refuses_to_run() {
    let folder = tempfile::tempdir().unwrap();
    let copy = folder
        .path()
        .join(Target::host().unwrap().executable_name("AiDEX"));
    fs::copy(LAUNCHER, &copy).unwrap();
    let output = Command::new(&copy).output().unwrap();
    assert_eq!(output.status.code(), Some(4), "{}", stderr(&output));
    assert!(stderr(&output).contains("not on an initialized AiDEX drive"));
}

#[test]
fn a_launcher_copied_elsewhere_on_the_drive_refuses_to_run() {
    let drive = Drive::new();
    let copy = drive
        .path()
        .join("workspaces")
        .join(drive.target.executable_name("AiDEX"));
    fs::copy(drive.launcher(), &copy).unwrap();
    let output = Command::new(&copy).output().unwrap();
    assert_eq!(output.status.code(), Some(4), "{}", stderr(&output));
    assert!(stderr(&output).contains("not at a launcher location"));
}

#[test]
fn launcher_info_reports_the_verified_bundle_without_running_it() {
    let drive = Drive::new();
    let output = drive.run(&["--launcher-info"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout_json(&output);
    assert_eq!(report["ok"], true);
    assert_eq!(report["marker"], LAUNCHER_MARKER);
    assert_eq!(report["target"], drive.target.as_str());
    assert_eq!(report["bundle"]["version"], "0.1.0");
    assert_eq!(report["bundle"]["scope"], "critical");
    assert_eq!(report["bundle"]["verified_files"], 4);
    assert_eq!(report["bundle"]["signature"]["status"], "verified");
    assert_eq!(report["bundle"]["signature"]["key_id"], drive.key.key_id());
    assert_eq!(report["bundle"]["signature"]["key_source"], "development");
    assert_eq!(report["dev_trust"], true);
    assert_eq!(report["pending_update"], Value::Null);
    same_dir(&report["drive_root"], drive.path());

    let output = drive.run(&["--launcher-verify"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout_json(&output)["bundle"]["scope"], "all");

    for bad in [&["--launcher-bogus"][..], &["--launcher-info", "x"]] {
        let output = drive.run(bad);
        assert_eq!(output.status.code(), Some(2), "{bad:?}");
    }
}

#[test]
fn an_unsigned_bundle_is_refused_unless_the_host_opts_in() {
    let drive = Drive::with_bundle(false);
    let output = drive.run(&["exit", "0"]);
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));
    assert!(output.stdout.is_empty(), "the CLI must not run");
    let message = stderr(&output);
    assert!(message.contains("not signed"), "{message}");
    assert!(message.contains("AIDEX_ALLOW_UNSIGNED"), "{message}");

    let output = drive.run_with(&["exit", "5"], &[("AIDEX_ALLOW_UNSIGNED", "1")]);
    assert_eq!(output.status.code(), Some(5), "{}", stderr(&output));
    assert!(stderr(&output).contains("UNSIGNED"), "{}", stderr(&output));
    let output = drive.run_with(&["--launcher-info"], &[("AIDEX_ALLOW_UNSIGNED", "1")]);
    assert_eq!(
        stdout_json(&output)["bundle"]["signature"]["status"],
        "unsigned_allowed"
    );
    // Any other value is not an opt-in.
    let output = drive.run_with(&["exit", "0"], &[("AIDEX_ALLOW_UNSIGNED", "yes")]);
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));
}

#[test]
fn an_untrusted_or_revoked_signing_key_is_refused() {
    let drive = Drive::new();
    let output = drive.run_with(&["exit", "0"], &[(DEV_TRUSTED_KEYS_ENV, "")]);
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("does not trust"),
        "{}",
        stderr(&output)
    );
    // The host opt-in for unsigned bundles never accepts an untrusted signature.
    let output = drive.run_with(
        &["exit", "0"],
        &[(DEV_TRUSTED_KEYS_ENV, ""), ("AIDEX_ALLOW_UNSIGNED", "1")],
    );
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));

    let mut list = RevocationList::default();
    list.add(Revocation {
        key_id: drive.key.key_id().into(),
        revoked_at: None,
        reason: Some("compromised".into()),
    });
    list.store(&revocations_path(&drive.paths())).unwrap();
    let output = drive.run(&["exit", "0"]);
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));
    assert!(stderr(&output).contains("revoked"), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
}

#[test]
fn a_re_pinned_tampered_manifest_fails_the_signature() {
    let drive = Drive::new();
    let manifest_path = drive.bundle_dir.join(MANIFEST_FILE_NAME);
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["rustc"] = "rustc (attacker)".into();
    let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    fs::write(&manifest_path, &bytes).unwrap();
    drive.edit_pointer(|entry| {
        entry["manifest_sha256"] = aidex_bundle::sha256_hex(&bytes).into();
    });
    let output = drive.run(&["exit", "0"]);
    assert_eq!(output.status.code(), Some(11), "{}", stderr(&output));
    assert!(stderr(&output).contains("signature"), "{}", stderr(&output));
    assert!(output.stdout.is_empty());
}

#[test]
fn an_interrupted_update_is_reported_and_the_launcher_still_writes_nothing() {
    let drive = Drive::new();
    let paths = drive.paths();
    let before = CurrentPointer::load(&layout::pointer_path(&paths)).unwrap();
    let mut journal = UpdateJournal::new(
        aidex_core::EntityId::generate(UPDATE_ID_PREFIX).unwrap(),
        JournalOperation::Apply,
        drive.target,
        JournalState::SelfTest,
        before,
    );
    journal.advance(JournalState::Switching);
    journal.store(&journal_path(&paths)).unwrap();
    let snapshot_before = snapshot(drive.path());
    let output = drive.run(&["--launcher-info"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout_json(&output)["pending_update"], "switching");
    assert!(
        stderr(&output).contains("interrupted"),
        "{}",
        stderr(&output)
    );
    let output = drive.run(&["exit", "4"]);
    assert_eq!(output.status.code(), Some(4), "{}", stderr(&output));
    assert_eq!(snapshot(drive.path()), snapshot_before);
}

/// Every path under `root`, with size and modification time for files. (Directory
/// timestamps are skipped: Windows reports them from a lazily updated cache.)
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<(u64, Option<std::time::SystemTime>)>> {
    let mut entries = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            let facts = if metadata.is_dir() {
                stack.push(path.clone());
                None
            } else {
                Some((metadata.len(), metadata.modified().ok()))
            };
            entries.insert(path, facts);
        }
    }
    entries
}
