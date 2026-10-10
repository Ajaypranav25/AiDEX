//! `aidex update …` end to end: the CLI binary against an in-process daemon on a
//! temporary drive, with signing keys generated at run time.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
};

use aidex_bundle::{
    BundleFile, BundleManifest, FileRole, KeyPair, KeySource, LAUNCHER_MARKER, MANIFEST_FILE_NAME,
    MANIFEST_SCHEMA_VERSION, PRODUCT_NAME, StatePaths, Target, TrustStore, VerifyScope,
    archive::{ArchiveFormat, archive_file_name, bundle_root_name, write_archive},
    layout, read_manifest_bytes, sign_manifest, verify_bundle, write_signature,
};
use aidex_updater::{SelfTestFn, SelfTestReport, UpdaterConfig};
use aidexd::{Daemon, DaemonHandle, DaemonOptions};
use serde_json::Value;

struct Fixture {
    root: tempfile::TempDir,
    out: tempfile::TempDir,
    key: KeyPair,
    _daemon: DaemonHandle,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        aidex_drive::init_with(
            root.path(),
            aidex_drive::InitOptions {
                require_removable: false,
            },
        )
        .unwrap();
        let key = KeyPair::generate("cli-tests").unwrap();
        let trust = TrustStore::empty().with_key(key.trusted_key(KeySource::Explicit));
        let config = UpdaterConfig::new(trust).with_self_test(Arc::new(SelfTestFn(
            |_: &Path, _: &BundleManifest, _: &Path| Ok(SelfTestReport::default()),
        )));
        let options = DaemonOptions {
            updater: Some(config),
            ..DaemonOptions::default()
        };
        let daemon = Daemon::with_options(root.path(), options)
            .unwrap()
            .start()
            .unwrap();
        Self {
            root,
            out: tempfile::tempdir().unwrap(),
            key,
            _daemon: daemon,
        }
    }

    /// Run `aidex --json <args>` on the drive (as the launcher would, through
    /// AIDEX_DRIVE_ROOT), trusting the test key.
    fn aidex(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aidex"))
            .arg("--json")
            .args(args)
            .env("AIDEX_DRIVE_ROOT", self.root.path())
            .env_remove("AIDEX_ALLOW_UNSIGNED")
            .env("AIDEX_OMP", "aidex-test-missing-omp-executable")
            .env(
                "AIDEX_DEV_TRUSTED_KEYS",
                format!("cli-tests={}", self.key.public_key_hex()),
            )
            .output()
            .unwrap()
    }

    fn package(&self, version: &str, signed: bool) -> PathBuf {
        let target = Target::host().unwrap();
        let out = self.out.path().join(version);
        let dir = out.join("src").join(bundle_root_name(version, target));
        fs::create_dir_all(dir.join("bin")).unwrap();
        let mut files = Vec::new();
        for (role, stem) in [
            (FileRole::Launcher, layout::LAUNCHER_BINARY),
            (FileRole::Cli, layout::CLI_BINARY),
            (FileRole::Daemon, layout::DAEMON_BINARY),
            (FileRole::Helper, layout::HELPER_BINARY),
        ] {
            let path = layout::bin_path(stem, target);
            fs::write(
                dir.join(&path),
                format!("{stem} {version}\n{LAUNCHER_MARKER}\n"),
            )
            .unwrap();
            files.push(BundleFile::describe(&dir, &path, role, true).unwrap());
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
        if signed {
            let bytes = read_manifest_bytes(&dir).unwrap();
            write_signature(&dir, &sign_manifest(&bytes, &self.key)).unwrap();
        }
        let path = out.join(archive_file_name(version, target));
        write_archive(
            &verify_bundle(&dir, VerifyScope::All).unwrap(),
            &path,
            ArchiveFormat::for_target(target),
        )
        .unwrap();
        path
    }
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
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

fn stderr_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|error| {
        panic!(
            "stderr is not JSON ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn apply_status_verify_rollback_and_refusals_through_the_cli() {
    let fixture = Fixture::new();
    let v1 = fixture.package("0.1.0", true);
    let v2 = fixture.package("0.2.0", true);

    let output = fixture.aidex(&["update", "check", v1.to_str().unwrap()]);
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout_json(&output)["action"], "install");

    for package in [&v1, &v2] {
        let output = fixture.aidex(&["update", "apply", package.to_str().unwrap()]);
        assert_eq!(
            code(&output),
            0,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = stdout_json(&output);
        assert_eq!(report["restart_required"], true);
        assert_eq!(
            report["candidate"]["signature"]["key_id"],
            fixture.key.key_id()
        );
    }

    let output = fixture.aidex(&["update", "status"]);
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let status = stdout_json(&output);
    assert_eq!(status["current"]["version"], "0.2.0");
    assert_eq!(status["previous"]["version"], "0.1.0");
    assert_eq!(status["integrity"]["verified"], true);

    let output = fixture.aidex(&["update", "verify"]);
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let verified = stdout_json(&output);
    assert_eq!(verified["scope"], "all");
    assert_eq!(verified["signer_key_id"], fixture.key.key_id());

    // doctor reports the installed bundle's integrity.
    let output = fixture.aidex(&["doctor"]);
    let doctor = stdout_json(&output);
    assert_eq!(doctor["integrity"]["verified"], true, "{doctor}");
    assert_eq!(doctor["integrity"]["version"], "0.2.0");

    // An unsigned package is an integrity refusal (exit 11); a downgrade is exit 10.
    let unsigned = fixture.package("0.3.0", false);
    let output = fixture.aidex(&["update", "apply", unsigned.to_str().unwrap()]);
    assert_eq!(code(&output), 11);
    assert_eq!(stderr_json(&output)["error"]["code"], "integrity_failed");
    let output = fixture.aidex(&["update", "apply", v1.to_str().unwrap()]);
    assert_eq!(code(&output), 10);

    let output = fixture.aidex(&["update", "rollback"]);
    assert_eq!(
        code(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout_json(&output)["to"]["version"], "0.1.0");

    // Tampering with the installed bundle makes verify fail with exit 11.
    let target = Target::host().unwrap();
    let cli = fixture
        .root
        .path()
        .join("app/versions")
        .join(format!("0.1.0-{target}"))
        .join(layout::bin_path("aidex", target));
    fs::write(&cli, "tampered").unwrap();
    let output = fixture.aidex(&["update", "verify"]);
    assert_eq!(code(&output), 11);
    assert_eq!(stderr_json(&output)["error"]["code"], "integrity_failed");
    let output = fixture.aidex(&["doctor"]);
    assert_eq!(stdout_json(&output)["integrity"]["verified"], false);
    assert_eq!(code(&output), 1);
}
