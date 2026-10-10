//! `bundle.sig`: signed installs, and what `resolve_active` refuses when signatures are
//! required (the launcher's default).

mod common;

use std::fs;

use aidex_bundle::{
    BundleError, BundleErrorKind, InstallSource, KeyPair, KeySource, MANIFEST_FILE_NAME,
    Revocation, RevocationList, SIGNATURE_FILE_NAME, SignatureError, SignaturePolicy,
    SignatureState, TrustStore, VerifyScope, install, install_signed, layout, read_signature,
    resolve_active, revocations_path, verify_bundle,
};
use aidex_drive::DrivePaths;
use serde_json::Value;

struct Signed {
    drive: tempfile::TempDir,
    _source: tempfile::TempDir,
    key: KeyPair,
    dir: std::path::PathBuf,
}

impl Signed {
    fn new() -> Self {
        let key = KeyPair::generate("bundle-tests").unwrap();
        let drive = common::temp_drive();
        let (source, bundle) = common::bundle(common::host(), "0.1.0", "a");
        common::sign(&bundle, &key);
        let report = install_signed(
            &InstallSource::Directory(bundle),
            drive.path(),
            &Self::policy_for(&key),
            &mut |_| Ok(()),
        )
        .unwrap();
        assert!(matches!(
            report.signature,
            Some(SignatureState::Verified { .. })
        ));
        let dir = drive.path().join("app").join(&report.dir);
        Self {
            drive,
            _source: source,
            key,
            dir,
        }
    }

    fn policy_for(key: &KeyPair) -> SignaturePolicy {
        SignaturePolicy::require(TrustStore::empty().with_key(key.trusted_key(KeySource::Explicit)))
    }

    fn paths(&self) -> DrivePaths {
        DrivePaths::open(self.drive.path()).unwrap()
    }

    fn resolve(&self, policy: &SignaturePolicy) -> Result<aidex_bundle::ActiveBundle, BundleError> {
        resolve_active(&self.paths(), common::host(), VerifyScope::Critical, policy)
    }

    fn signature_error(&self, policy: &SignaturePolicy) -> SignatureError {
        match self.resolve(policy).unwrap_err() {
            error @ BundleError::Signature { .. } => {
                assert_eq!(error.kind(), BundleErrorKind::Untrusted);
                let BundleError::Signature { error, .. } = error else {
                    unreachable!()
                };
                error
            }
            other => panic!("expected a signature error, got {other}"),
        }
    }

    /// Rewrite `bundle.json` and re-pin it in the pointer, as an attacker who can write
    /// the drive would, so only the signature can notice.
    fn tamper_manifest(&self) {
        let path = self.dir.join(MANIFEST_FILE_NAME);
        let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["rustc"] = "rustc 9.9.9 (attacker)".into();
        let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        fs::write(&path, &bytes).unwrap();
        let pointer = layout::pointer_path(&self.paths());
        let mut value: Value = serde_json::from_slice(&fs::read(&pointer).unwrap()).unwrap();
        value["targets"][common::host().as_str()]["manifest_sha256"] =
            aidex_bundle::sha256_hex(&bytes).into();
        fs::write(&pointer, value.to_string()).unwrap();
    }
}

#[test]
fn a_signed_bundle_resolves_with_its_key_and_records_the_signer() {
    let signed = Signed::new();
    let active = signed.resolve(&Signed::policy_for(&signed.key)).unwrap();
    assert_eq!(
        active.signature,
        SignatureState::Verified {
            key_id: signed.key.key_id().into(),
            key_name: "bundle-tests".into(),
            key_source: KeySource::Explicit,
        }
    );
    // The signature is allowed next to bundle.json even under full verification.
    verify_bundle(&signed.dir, VerifyScope::All).unwrap();
}

#[test]
fn an_unsigned_bundle_is_refused_unless_explicitly_allowed() {
    let drive = common::temp_drive();
    let (_source, bundle) = common::bundle(common::host(), "0.1.0", "a");
    let key = KeyPair::generate("k").unwrap();
    let error = install_signed(
        &InstallSource::Directory(bundle.clone()),
        drive.path(),
        &Signed::policy_for(&key),
        &mut |_| Ok(()),
    )
    .unwrap_err();
    assert!(
        matches!(
            error,
            BundleError::Signature {
                error: SignatureError::Unsigned,
                ..
            }
        ),
        "{error}"
    );
    assert!(common::version_dirs(drive.path()).is_empty());

    install(&InstallSource::Directory(bundle), drive.path()).unwrap();
    let paths = DrivePaths::open(drive.path()).unwrap();
    let error = resolve_active(
        &paths,
        common::host(),
        VerifyScope::Critical,
        &Signed::policy_for(&key),
    )
    .unwrap_err();
    assert_eq!(error.kind(), BundleErrorKind::Untrusted, "{error}");
    let allowed = resolve_active(
        &paths,
        common::host(),
        VerifyScope::Critical,
        &SignaturePolicy::allow_unsigned(TrustStore::empty()),
    )
    .unwrap();
    assert_eq!(allowed.signature, SignatureState::UnsignedAllowed);
}

#[test]
fn a_tampered_manifest_fails_the_signature_even_when_re_pinned() {
    let signed = Signed::new();
    signed.tamper_manifest();
    assert!(matches!(
        signed.signature_error(&Signed::policy_for(&signed.key)),
        SignatureError::ManifestMismatch { .. }
    ));
    // Also rewriting bundle.sig's digest leaves a signature that does not verify.
    let sig_path = signed.dir.join(SIGNATURE_FILE_NAME);
    let mut sig: Value = serde_json::from_slice(&fs::read(&sig_path).unwrap()).unwrap();
    sig["manifest_sha256"] =
        aidex_bundle::sha256_hex(&fs::read(signed.dir.join(MANIFEST_FILE_NAME)).unwrap()).into();
    fs::write(&sig_path, sig.to_string()).unwrap();
    assert!(matches!(
        signed.signature_error(&Signed::policy_for(&signed.key)),
        SignatureError::BadSignature { .. }
    ));
    // Allowing unsigned bundles never accepts a bad signature.
    let mut lenient = Signed::policy_for(&signed.key);
    lenient.allow_unsigned = true;
    assert!(matches!(
        signed.signature_error(&lenient),
        SignatureError::BadSignature { .. }
    ));
}

#[test]
fn unknown_and_revoked_keys_are_refused() {
    let signed = Signed::new();
    let stranger = KeyPair::generate("stranger").unwrap();
    assert!(matches!(
        signed.signature_error(&Signed::policy_for(&stranger)),
        SignatureError::UnknownKey { .. }
    ));
    let mut list = RevocationList::default();
    list.add(Revocation {
        key_id: signed.key.key_id().into(),
        revoked_at: None,
        reason: Some("lost laptop".into()),
    });
    let paths = signed.paths();
    list.store(&revocations_path(&paths)).unwrap();
    let trust = TrustStore::empty()
        .with_key(signed.key.trusted_key(KeySource::Explicit))
        .with_drive_revocations(&paths)
        .unwrap();
    assert!(matches!(
        signed.signature_error(&SignaturePolicy::require(trust)),
        SignatureError::RevokedKey {
            reason: Some(_),
            ..
        }
    ));
}

#[test]
fn a_malformed_or_linked_signature_file_is_refused() {
    let signed = Signed::new();
    fs::write(signed.dir.join(SIGNATURE_FILE_NAME), b"{not json").unwrap();
    assert!(matches!(
        signed.signature_error(&Signed::policy_for(&signed.key)),
        SignatureError::Malformed(_)
    ));
    fs::remove_file(signed.dir.join(SIGNATURE_FILE_NAME)).unwrap();
    fs::create_dir(signed.dir.join(SIGNATURE_FILE_NAME)).unwrap();
    assert!(read_signature(&signed.dir).is_err());
}

#[test]
fn a_manifest_may_not_list_the_signature_file() {
    let (_source, dir) = common::bundle(common::host(), "0.1.0", "a");
    let mut manifest = aidex_bundle::read_manifest(&dir).unwrap().0;
    let mut entry = manifest.files[0].clone();
    entry.path = SIGNATURE_FILE_NAME.into();
    manifest.files.push(entry);
    let error = manifest.validate().unwrap_err().to_string();
    assert!(error.contains("reserved"), "{error}");
}
