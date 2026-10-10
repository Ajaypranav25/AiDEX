//! `keygen`, `sign`, and `verify`, and the trust policy of `install` and `smoke`.
//!
//! Keys made here are development or CI keys. A release signing key never exists as
//! a file in a checkout or a CI workspace that pull requests can reach; see
//! `portable/packaging/README.md#signing-keys`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use aidex_bundle::{
    KeyPair, KeySource, PublicKeyFile, SHA256SUMS_FILE_NAME, Sha256Sums, SignaturePolicy,
    SignatureState, Target, TrustStore, VerifyScope,
    archive::{ArchiveFormat, ExtractLimits, extract_archive, write_archive},
    read_manifest_bytes, sign_manifest, verify_bundle, verify_signature, write_signature,
};
use aidex_updater::{SbomStatus, sbom_status};
use serde::Serialize;

use crate::{
    cli::{KeygenArgs, SignArgs, VerifyArgs},
    error::PackagerError,
};

#[derive(Debug, Serialize)]
pub struct KeygenReport {
    pub name: String,
    pub key_id: String,
    pub public_key: String,
    /// Secret key file (mode 0600 on Unix). Never commit or upload it.
    pub secret_key_file: PathBuf,
    pub public_key_file: PathBuf,
}

/// Generate a development/CI key pair as `<out>/<name>.key` and `<out>/<name>.pub.json`.
pub fn keygen(args: &KeygenArgs) -> Result<KeygenReport, PackagerError> {
    fs::create_dir_all(&args.out).map_err(|error| {
        PackagerError::io(
            format_args!("cannot create `{}`", args.out.display()),
            error,
        )
    })?;
    let pair = KeyPair::generate(&args.name)?;
    let secret_key_file = args.out.join(format!("{}.key", args.name));
    let public_key_file = args.out.join(format!("{}.pub.json", args.name));
    if public_key_file.exists() {
        return Err(PackagerError::new(format!(
            "`{}` already exists; refusing to overwrite a key file",
            public_key_file.display()
        )));
    }
    pair.store_secret(&secret_key_file)?;
    pair.public_file().store(&public_key_file)?;
    Ok(KeygenReport {
        name: args.name.clone(),
        key_id: pair.key_id().to_owned(),
        public_key: pair.public_key_hex(),
        secret_key_file,
        public_key_file,
    })
}

#[derive(Debug, Serialize)]
pub struct SignReport {
    pub bundle: PathBuf,
    pub version: String,
    pub target: Target,
    pub key_id: String,
    pub key_name: String,
    pub manifest_sha256: String,
    /// The rewritten archive's digest (archives only).
    pub archive_sha256: Option<String>,
    /// Whether `SHA256SUMS` next to the archive was updated.
    pub sha256sums_updated: bool,
}

/// Sign a bundle directory (writes `bundle.sig`) or archive (rewritten with its
/// `bundle.sig`; its `SHA256SUMS` line is updated when that file exists).
pub fn sign(args: &SignArgs) -> Result<SignReport, PackagerError> {
    let key = KeyPair::load_secret(&args.key)?;
    if args.bundle.is_dir() {
        let bundle = verify_bundle(&args.bundle, VerifyScope::All)?;
        let bytes = read_manifest_bytes(&args.bundle)?;
        write_signature(&args.bundle, &sign_manifest(&bytes, &key))?;
        return Ok(SignReport {
            bundle: args.bundle.clone(),
            version: bundle.manifest.version,
            target: bundle.manifest.target,
            key_id: key.key_id().to_owned(),
            key_name: key.name().to_owned(),
            manifest_sha256: bundle.manifest_sha256,
            archive_sha256: None,
            sha256sums_updated: false,
        });
    }
    let format = ArchiveFormat::detect(&args.bundle).ok_or_else(|| {
        PackagerError::new(format!(
            "`{}` is neither a bundle directory nor a .zip/.tar.gz archive",
            args.bundle.display()
        ))
    })?;
    let work = WorkDir::next_to(&args.bundle, "sign")?;
    let dir = extract_archive(&args.bundle, work.path(), &ExtractLimits::default())?;
    let bundle = verify_bundle(&dir, VerifyScope::All)?;
    let bytes = read_manifest_bytes(&dir)?;
    write_signature(&dir, &sign_manifest(&bytes, &key))?;
    let bundle_with_signature = verify_bundle(&dir, VerifyScope::All)?;
    let summary = write_archive(&bundle_with_signature, &args.bundle, format)?;
    let sums_path = args
        .bundle
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SHA256SUMS_FILE_NAME);
    let mut sha256sums_updated = false;
    if sums_path.is_file() {
        let name = file_name(&args.bundle)?;
        let mut sums = Sha256Sums::load(&sums_path)?;
        sums.set(&name, &summary.sha256)
            .map_err(PackagerError::new)?;
        sums.store(&sums_path)?;
        sha256sums_updated = true;
    }
    Ok(SignReport {
        bundle: args.bundle.clone(),
        version: bundle.manifest.version,
        target: bundle.manifest.target,
        key_id: key.key_id().to_owned(),
        key_name: key.name().to_owned(),
        manifest_sha256: bundle.manifest_sha256,
        archive_sha256: Some(summary.sha256),
        sha256sums_updated,
    })
}

#[derive(Debug, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub bundle: PathBuf,
    pub version: String,
    pub target: Target,
    pub manifest_sha256: String,
    pub signature: SignatureState,
    pub verified_files: usize,
    /// `verified`, `absent` (no SHA256SUMS next to the archive), or `not_applicable`.
    pub checksum: &'static str,
    pub sbom: SbomStatus,
}

/// Verify a bundle directory or archive: checksum, every file, and the signature.
pub fn verify(args: &VerifyArgs) -> Result<VerifyReport, PackagerError> {
    let policy = policy(&args.trust_key, args.allow_unsigned)?;
    let (dir, checksum, _work) = if args.bundle.is_dir() {
        (args.bundle.clone(), "not_applicable", None)
    } else {
        let checksum = check_sums(&args.bundle)?;
        let work = WorkDir::next_to(&args.bundle, "verify")?;
        let dir = extract_archive(&args.bundle, work.path(), &ExtractLimits::default())?;
        (dir, checksum, Some(work))
    };
    let bundle = verify_bundle(&dir, VerifyScope::All)?;
    let bytes = read_manifest_bytes(&dir)?;
    let signature = verify_signature(&dir, &bytes, &bundle.manifest.created_at, &policy)?;
    let sbom = sbom_status(&dir, &bundle.manifest);
    Ok(VerifyReport {
        ok: true,
        bundle: args.bundle.clone(),
        version: bundle.manifest.version.clone(),
        target: bundle.manifest.target,
        manifest_sha256: bundle.manifest_sha256,
        signature,
        verified_files: bundle.verified_files,
        checksum,
        sbom,
    })
}

/// The built-in trust store plus `--trust-key` public key files.
pub fn policy(
    trust_keys: &[PathBuf],
    allow_unsigned: bool,
) -> Result<SignaturePolicy, PackagerError> {
    let mut trust = TrustStore::builtin()?;
    for path in trust_keys {
        let file = PublicKeyFile::load(path)?;
        let key = file
            .to_trusted(KeySource::Explicit)
            .map_err(|error| PackagerError::new(format!("`{}`: {error}", path.display())))?;
        trust.add_key(key);
    }
    if allow_unsigned {
        eprintln!(
            "aidex-packager: warning: --allow-unsigned accepts bundles without a signature (development only)"
        );
    }
    Ok(SignaturePolicy {
        trust,
        allow_unsigned,
    })
}

/// `name=hex` entries for `AIDEX_BUILD_TRUSTED_KEYS` from public key files.
pub fn build_trust_env(trust_keys: &[PathBuf]) -> Result<Option<String>, PackagerError> {
    let mut entries = Vec::new();
    for path in trust_keys {
        let file = PublicKeyFile::load(path)?;
        entries.push(format!("{}={}", file.name, file.public_key));
    }
    Ok((!entries.is_empty()).then(|| entries.join(",")))
}

fn check_sums(archive: &Path) -> Result<&'static str, PackagerError> {
    let sums_path = archive
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SHA256SUMS_FILE_NAME);
    if !sums_path.is_file() {
        return Ok("absent");
    }
    let name = file_name(archive)?;
    let expected = Sha256Sums::load(&sums_path)?
        .get(&name)
        .map(str::to_owned)
        .ok_or_else(|| PackagerError::new(format!("SHA256SUMS does not list `{name}`")))?;
    let (actual, _) = aidex_bundle::sha256_file(archive).map_err(|error| {
        PackagerError::io(format_args!("cannot read `{}`", archive.display()), error)
    })?;
    if actual != expected {
        return Err(PackagerError::new(format!(
            "`{name}` has sha256 {actual}, but SHA256SUMS lists {expected}"
        )));
    }
    Ok("verified")
}

fn file_name(path: &Path) -> Result<String, PackagerError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| PackagerError::new(format!("`{}` has no UTF-8 file name", path.display())))
}

/// A scratch directory next to `path`, removed on drop.
struct WorkDir(PathBuf);

impl WorkDir {
    fn next_to(path: &Path, tag: &str) -> Result<Self, PackagerError> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let dir = parent.join(format!(".aidex-packager-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).map_err(|error| {
            PackagerError::io(format_args!("cannot create `{}`", dir.display()), error)
        })?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
