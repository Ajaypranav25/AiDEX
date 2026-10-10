//! Acceptance: vault records are encrypted at rest and open on another host with the
//! user secret (passphrase or recovery key).

mod common;

use aidex_vault::{
    CreateOptions, RecoveryKey, SecretBytes, SecretKind, UnlockMethod, Vault, VaultErrorKind,
};
use common::*;

#[test]
fn vault_reopens_from_a_copied_directory_on_another_host() {
    let host_a = tempfile::tempdir().unwrap();
    let (dir, vault, _recovery) = create_with_secret(host_a.path());
    let vault_id = vault.vault_id().clone();
    let locked = vault.lock();
    assert_eq!(locked.vault_id(), &vault_id);

    // Simulate carrying the drive to another machine: a byte-for-byte copy elsewhere.
    let host_b = tempfile::tempdir().unwrap();
    let copied = vault_dir(host_b.path());
    copy_dir(&dir, &copied);
    assert!(Vault::exists(&copied));
    let locked = Vault::open(&copied).unwrap();
    assert_eq!(locked.vault_id(), &vault_id);
    assert!(locked.has_recovery_key());
    assert_eq!(locked.kdf_params(), tiny());
    let mut reopened = locked.unlock(&passphrase(PASSPHRASE)).unwrap();
    assert_eq!(reopened.unlock_method(), UnlockMethod::Passphrase);
    assert_eq!(reveal(&mut reopened, NAME), VALUE.as_bytes());
    let listed = reopened.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name.as_str(), NAME);
    assert_eq!(listed[0].kind, SecretKind::ApiKey);
    assert_eq!(listed[0].secret_ref.vault_id, vault_id);
    let report = reopened.verify().unwrap();
    assert_eq!(report.records, 1);
}

#[test]
fn recovery_key_unlocks_and_resets_a_forgotten_passphrase() {
    let root = tempfile::tempdir().unwrap();
    let (dir, vault, recovery) = create_with_secret(root.path());
    drop(vault);
    let text = recovery.encode();
    let typed = RecoveryKey::parse(&text.expose().to_ascii_lowercase()).unwrap();

    let locked = Vault::open(&dir).unwrap();
    let mut vault = locked.unlock_with_recovery(&typed).unwrap();
    assert_eq!(vault.unlock_method(), UnlockMethod::RecoveryKey);
    assert_eq!(reveal(&mut vault, NAME), VALUE.as_bytes());
    vault
        .reset_passphrase(&passphrase("a brand new passphrase"))
        .unwrap();
    drop(vault);

    let locked = Vault::open(&dir).unwrap();
    assert_eq!(
        locked.unlock(&passphrase(PASSPHRASE)).unwrap_err().kind(),
        VaultErrorKind::WrongPassphrase
    );
    let mut vault = locked
        .unlock(&passphrase("a brand new passphrase"))
        .unwrap();
    assert_eq!(reveal(&mut vault, NAME), VALUE.as_bytes());
    assert_eq!(
        vault
            .reset_passphrase(&passphrase("passphrase unlock cannot reset"))
            .unwrap_err()
            .kind(),
        VaultErrorKind::PassphraseRequired
    );
    // The recovery key still works after the passphrase changed.
    drop(vault);
    Vault::open(&dir)
        .unwrap()
        .unlock_with_recovery(&recovery)
        .unwrap();
}

#[test]
fn wrong_or_missing_recovery_keys_are_refused() {
    let root = tempfile::tempdir().unwrap();
    let (dir, vault, _recovery) = create_with_secret(root.path());
    drop(vault);
    let other_root = tempfile::tempdir().unwrap();
    let (_, other, other_recovery) = create_with_secret(other_root.path());
    drop(other);
    let error = Vault::open(&dir)
        .unwrap()
        .unlock_with_recovery(&other_recovery)
        .unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::WrongRecoveryKey);

    let plain = tempfile::tempdir().unwrap();
    let (vault, recovery) = Vault::create(
        &vault_dir(plain.path()),
        &passphrase(PASSPHRASE),
        CreateOptions {
            kdf: tiny(),
            recovery_key: false,
        },
    )
    .unwrap();
    assert!(recovery.is_none() && !vault.has_recovery_key());
    drop(vault);
    let error = Vault::open(&vault_dir(plain.path()))
        .unwrap()
        .unlock_with_recovery(&other_recovery)
        .unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::NoRecoveryKey);
}

#[test]
fn passphrase_change_requires_the_old_passphrase() {
    let root = tempfile::tempdir().unwrap();
    let (dir, mut vault, _recovery) = create_with_secret(root.path());
    let error = vault
        .change_passphrase(
            &passphrase("not the passphrase"),
            &passphrase("new passphrase 1"),
        )
        .unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::WrongPassphrase);
    let error = vault
        .change_passphrase(&passphrase(PASSPHRASE), &passphrase("short"))
        .unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::InvalidArgument);
    vault
        .change_passphrase(&passphrase(PASSPHRASE), &passphrase("new passphrase 1"))
        .unwrap();
    assert_eq!(reveal(&mut vault, NAME), VALUE.as_bytes());
    drop(vault);
    let locked = Vault::open(&dir).unwrap();
    assert!(locked.unlock(&passphrase(PASSPHRASE)).is_err());
    let mut vault = locked.unlock(&passphrase("new passphrase 1")).unwrap();
    assert_eq!(reveal(&mut vault, NAME), VALUE.as_bytes());
}

#[test]
fn secrets_can_be_replaced_and_deleted_durably() {
    let root = tempfile::tempdir().unwrap();
    let (dir, mut vault, _recovery) = create_with_secret(root.path());
    let first = vault.meta(&name(NAME)).unwrap();
    let updated = vault
        .put(
            &name(NAME),
            SecretKind::Token,
            SecretBytes::from(b"replacement-value".to_vec()),
            policy(&[openai(), github_mcp()]),
        )
        .unwrap();
    assert_eq!(
        updated.secret_ref, first.secret_ref,
        "replacing keeps the secret id"
    );
    assert_eq!(updated.created_at, first.created_at);
    assert_eq!(updated.kind, SecretKind::Token);
    assert_eq!(
        record_files(&dir).len(),
        1,
        "the superseded record file is removed"
    );
    vault
        .put(
            &name("second"),
            SecretKind::Generic,
            SecretBytes::from(b"second-value".to_vec()),
            policy(&[]),
        )
        .unwrap();
    assert_eq!(
        vault
            .list()
            .iter()
            .map(|meta| meta.name.as_str())
            .collect::<Vec<_>>(),
        [NAME, "second"]
    );
    drop(vault);

    let mut vault = unlock(&dir);
    assert_eq!(reveal(&mut vault, NAME), b"replacement-value");
    let deleted = vault.delete(&name(NAME)).unwrap();
    assert_eq!(deleted.secret_ref, first.secret_ref);
    assert_eq!(
        vault.delete(&name(NAME)).unwrap_err().kind(),
        VaultErrorKind::SecretNotFound
    );
    drop(vault);
    let vault = unlock(&dir);
    assert_eq!(vault.len(), 1);
    assert_eq!(record_files(&dir).len(), 1);
}

#[test]
fn invalid_values_and_existing_vaults_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let (dir, mut vault, _recovery) = create_with_secret(root.path());
    for (value, expected) in [
        (Vec::new(), VaultErrorKind::InvalidArgument),
        (
            vec![b'x'; aidex_vault::MAX_SECRET_BYTES + 1],
            VaultErrorKind::InvalidArgument,
        ),
    ] {
        let error = vault
            .put(
                &name("bad"),
                SecretKind::Generic,
                SecretBytes::from(value),
                policy(&[]),
            )
            .unwrap_err();
        assert_eq!(error.kind(), expected);
    }
    let error = Vault::create(&dir, &passphrase(PASSPHRASE), CreateOptions::default()).unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::AlreadyExists);
    let error = Vault::create(
        &vault_dir(tempfile::tempdir().unwrap().path()),
        &passphrase("2short"),
        CreateOptions::default(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::InvalidArgument);
    assert!(!Vault::exists(&root.path().join("absent")));
    assert_eq!(
        Vault::open(&root.path().join("absent")).unwrap_err().kind(),
        VaultErrorKind::NotFound
    );
}

#[test]
fn concurrent_sessions_detect_changes_instead_of_overwriting() {
    let root = tempfile::tempdir().unwrap();
    let (dir, mut first, _recovery) = create_with_secret(root.path());
    let mut second = unlock(&dir);
    first
        .put(
            &name("from-first"),
            SecretKind::Generic,
            SecretBytes::from(b"one".to_vec()),
            policy(&[]),
        )
        .unwrap();
    let error = second
        .put(
            &name("from-second"),
            SecretKind::Generic,
            SecretBytes::from(b"two".to_vec()),
            policy(&[]),
        )
        .unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::Conflict);
    drop((first, second));
    let vault = unlock(&dir);
    let names: Vec<_> = vault
        .list()
        .into_iter()
        .map(|meta| meta.name.to_string())
        .collect();
    assert_eq!(names, ["from-first", NAME]);
}
