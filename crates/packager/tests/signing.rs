//! `aidex-packager keygen | sign | verify | install` on a synthetic bundle: keys are
//! generated at run time, signatures are required by default, and secrets never reach
//! stdout.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use aidex_bundle::{
    BundleFile, BundleManifest, FileRole, LAUNCHER_MARKER, MANIFEST_FILE_NAME,
    MANIFEST_SCHEMA_VERSION, PRODUCT_NAME, SHA256SUMS_FILE_NAME, Sha256Sums, StatePaths, Target,
    VerifyScope,
    archive::{ArchiveFormat, archive_file_name, bundle_root_name, write_archive},
    layout, verify_bundle,
};
use serde_json::Value;

const PACKAGER: &str = env!("CARGO_BIN_EXE_aidex-packager");

fn packager(args: &[&str]) -> Output {
    Command::new(PACKAGER)
        .args(args)
        .env_remove("AIDEX_DEV_TRUSTED_KEYS")
        .output()
        .unwrap()
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "not JSON ({error}): {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            stderr(output)
        )
    })
}

/// A synthetic host bundle archive (with SHA256SUMS) in `out`.
fn archive(out: &Path) -> PathBuf {
    let target = Target::host().unwrap_or(Target::WindowsX64);
    let dir = out.join("src").join(bundle_root_name("0.4.0", target));
    fs::create_dir_all(dir.join("bin")).unwrap();
    let mut files = Vec::new();
    for (role, stem) in [
        (FileRole::Launcher, layout::LAUNCHER_BINARY),
        (FileRole::Cli, layout::CLI_BINARY),
        (FileRole::Daemon, layout::DAEMON_BINARY),
        (FileRole::Helper, layout::HELPER_BINARY),
    ] {
        let path = layout::bin_path(stem, target);
        fs::write(dir.join(&path), format!("{stem}\n{LAUNCHER_MARKER}\n")).unwrap();
        files.push(BundleFile::describe(&dir, &path, role, true).unwrap());
    }
    let mut manifest = BundleManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        product: PRODUCT_NAME.into(),
        version: "0.4.0".into(),
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
    let path = out.join(archive_file_name("0.4.0", target));
    let summary = write_archive(
        &verify_bundle(&dir, VerifyScope::All).unwrap(),
        &path,
        ArchiveFormat::for_target(target),
    )
    .unwrap();
    let mut sums = Sha256Sums::default();
    sums.set(path.file_name().unwrap().to_str().unwrap(), &summary.sha256)
        .unwrap();
    sums.store(&out.join(SHA256SUMS_FILE_NAME)).unwrap();
    path
}

#[test]
fn keys_sign_verify_and_install_round_trip_with_explicit_trust() {
    let temp = tempfile::tempdir().unwrap();
    let keys = temp.path().join("keys");
    let output = packager(&[
        "--json",
        "keygen",
        "--name",
        "ci-test",
        "--out",
        text(&keys),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let report = json(&output);
    let key_id = report["key_id"].as_str().unwrap().to_owned();
    let secret_file = keys.join("ci-test.key");
    let public_file = keys.join("ci-test.pub.json");
    assert!(secret_file.is_file() && public_file.is_file());
    let secret: Value = serde_json::from_slice(&fs::read(&secret_file).unwrap()).unwrap();
    let secret_hex = secret["secret_key"].as_str().unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret_hex));
    // Never overwrites a key.
    let again = packager(&["keygen", "--name", "ci-test", "--out", text(&keys)]);
    assert!(!again.status.success());

    let out = temp.path().join("dist");
    let package = archive(&out);
    let unsigned_sums = fs::read(out.join(SHA256SUMS_FILE_NAME)).unwrap();

    // Unsigned bundles are refused unless explicitly allowed.
    let verify = packager(&["verify", text(&package), "--trust-key", text(&public_file)]);
    assert!(!verify.status.success());
    assert!(
        stderr(&verify).contains("not signed"),
        "{}",
        stderr(&verify)
    );
    let verify = packager(&["--json", "verify", text(&package), "--allow-unsigned"]);
    assert!(verify.status.success(), "{}", stderr(&verify));
    assert_eq!(json(&verify)["signature"]["status"], "unsigned_allowed");

    let sign = packager(&[
        "--json",
        "sign",
        "--key",
        text(&secret_file),
        text(&package),
    ]);
    assert!(sign.status.success(), "{}", stderr(&sign));
    let signed = json(&sign);
    assert_eq!(signed["key_id"], key_id.as_str());
    assert_eq!(signed["sha256sums_updated"], true);
    assert_ne!(
        fs::read(out.join(SHA256SUMS_FILE_NAME)).unwrap(),
        unsigned_sums
    );

    let verify = packager(&[
        "--json",
        "verify",
        text(&package),
        "--trust-key",
        text(&public_file),
    ]);
    assert!(verify.status.success(), "{}", stderr(&verify));
    let verified = json(&verify);
    assert_eq!(verified["checksum"], "verified");
    assert_eq!(verified["signature"]["status"], "verified");
    assert_eq!(verified["signature"]["key_id"], key_id.as_str());
    // A key this build does not trust is refused.
    let untrusted = packager(&["verify", text(&package)]);
    assert!(!untrusted.status.success());
    assert!(
        stderr(&untrusted).contains("does not trust"),
        "{}",
        stderr(&untrusted)
    );

    let drive = temp.path().join("drive");
    fs::create_dir_all(&drive).unwrap();
    let refused = packager(&[
        "install",
        "--bundle",
        text(&package),
        "--drive",
        text(&drive),
        "--init-dev-drive",
    ]);
    assert!(!refused.status.success());
    let install = packager(&[
        "--json",
        "install",
        "--bundle",
        text(&package),
        "--drive",
        text(&drive),
        "--trust-key",
        text(&public_file),
    ]);
    assert!(install.status.success(), "{}", stderr(&install));
    assert_eq!(json(&install)["signature"]["key_id"], key_id.as_str());
}
