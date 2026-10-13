//! `VaultService`: the daemon-facing state machine, auto-lock, and broker handle API.

mod common;

use aidex_vault::{
    RecoveryKey, SecretBytes, SecretKind, UnlockMethod, VaultError, VaultErrorKind, VaultService,
    VaultServiceConfig, VaultState,
};
use common::*;
use std::{
    fs, thread,
    time::{Duration, Instant},
};

fn service(root: &std::path::Path, idle: Option<Duration>) -> VaultService {
    VaultService::open(
        vault_dir(root),
        VaultServiceConfig {
            kdf: tiny(),
            idle_timeout: idle,
        },
    )
    .unwrap()
}

#[test]
fn service_moves_between_missing_locked_and_unlocked() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(vault_dir(root.path())).unwrap();
    let service = service(root.path(), None);
    let status = service.status();
    assert_eq!(status.state, VaultState::Missing);
    assert_eq!(
        service.list_secrets().unwrap_err().kind(),
        VaultErrorKind::NotFound
    );

    let outcome = service.init(&passphrase(PASSPHRASE), true).unwrap();
    let recovery = outcome.recovery_key.expect("recovery requested");
    let status = service.status();
    assert_eq!(status.state, VaultState::Unlocked);
    assert_eq!(status.vault_id.as_ref(), Some(&outcome.vault_id));
    assert_eq!(status.secrets, Some(0));
    assert_eq!(status.unlocked_by, Some(UnlockMethod::Passphrase));
    assert_eq!(
        service
            .init(&passphrase(PASSPHRASE), false)
            .unwrap_err()
            .kind(),
        VaultErrorKind::AlreadyExists
    );

    service
        .put_secret(
            &name(NAME),
            SecretKind::ApiKey,
            SecretBytes::from(VALUE.to_owned()),
            policy(&[openai()]),
        )
        .unwrap();
    assert_eq!(service.list_secrets().unwrap().len(), 1);
    let handle = service
        .issue_handle(&name(NAME), &openai(), Duration::from_secs(60))
        .unwrap();
    assert_eq!(
        service.resolve(&handle, &openai()).unwrap().expose(),
        VALUE.as_bytes()
    );

    let outcome = service.lock();
    assert!(outcome.was_unlocked);
    assert_eq!(outcome.revoked_handles, 1);
    assert_eq!(service.status().state, VaultState::Locked);
    for error in [
        service.list_secrets().unwrap_err(),
        service.resolve(&handle, &openai()).unwrap_err(),
        service
            .issue_handle(&name(NAME), &openai(), Duration::from_secs(60))
            .unwrap_err(),
    ] {
        assert_eq!(error.kind(), VaultErrorKind::Locked);
    }
    assert!(!service.lock().was_unlocked);

    assert_eq!(
        service
            .unlock(&passphrase("wrong passphrase!"))
            .unwrap_err()
            .kind(),
        VaultErrorKind::WrongPassphrase
    );
    let status = service
        .unlock_with_recovery(&RecoveryKey::parse(recovery.encode().expose()).unwrap())
        .unwrap();
    assert_eq!(status.unlocked_by, Some(UnlockMethod::RecoveryKey));
    // The old handle died with the previous session.
    assert!(matches!(
        service.resolve(&handle, &openai()),
        Err(VaultError::HandleDenied(_))
    ));
    service
        .change_passphrase(None, &passphrase("reset via recovery"))
        .unwrap();
    service.lock();
    service.unlock(&passphrase("reset via recovery")).unwrap();
    assert_eq!(
        service
            .change_passphrase(None, &passphrase("needs the old one"))
            .unwrap_err()
            .kind(),
        VaultErrorKind::PassphraseRequired
    );
    service
        .change_passphrase(
            Some(&passphrase("reset via recovery")),
            &passphrase("final passphrase"),
        )
        .unwrap();
    service.delete_secret(&name(NAME)).unwrap();
    assert!(service.list_secrets().unwrap().is_empty());
}

#[test]
fn idle_vaults_lock_themselves() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(vault_dir(root.path())).unwrap();
    // Generous margins: CI runners can stall a thread for tens of milliseconds.
    let service = service(root.path(), Some(Duration::from_millis(1_000)));
    service.init(&passphrase(PASSPHRASE), false).unwrap();
    assert_eq!(service.status().state, VaultState::Unlocked);
    assert_eq!(service.status().idle_lock_secs, Some(1));

    // Use keeps it unlocked...
    for _ in 0..3 {
        thread::sleep(Duration::from_millis(150));
        service.list_secrets().unwrap();
    }
    assert_eq!(service.status().state, VaultState::Unlocked);
    // ...and the background thread locks it once idle.
    let deadline = Instant::now() + Duration::from_secs(10);
    while service.status().state == VaultState::Unlocked {
        assert!(Instant::now() < deadline, "vault did not auto-lock");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(service.status().state, VaultState::Locked);
    assert_eq!(
        service.list_secrets().unwrap_err().kind(),
        VaultErrorKind::Locked
    );
}

#[test]
fn damaged_vaults_are_reported_not_hidden() {
    let root = tempfile::tempdir().unwrap();
    let (dir, vault, _) = create_with_secret(root.path());
    drop(vault);
    let service = service(root.path(), None);
    assert_eq!(service.status().state, VaultState::Locked);
    fs::write(header_path(&dir), b"{ torn").unwrap();
    let status = service.status();
    assert_eq!(status.state, VaultState::Unavailable);
    assert_eq!(status.reason.as_deref(), Some("corrupt"));
    assert_eq!(
        service.unlock(&passphrase(PASSPHRASE)).unwrap_err().kind(),
        VaultErrorKind::Corrupt
    );
    assert_eq!(
        service
            .init(&passphrase(PASSPHRASE), false)
            .unwrap_err()
            .kind(),
        VaultErrorKind::AlreadyExists
    );
    fs::remove_file(header_path(&dir)).unwrap();
    assert_eq!(service.status().reason.as_deref(), Some("missing_header"));
}

#[test]
fn external_changes_end_the_session_instead_of_being_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let (dir, vault, _) = create_with_secret(root.path());
    drop(vault);
    let service = service(root.path(), None);
    service.unlock(&passphrase(PASSPHRASE)).unwrap();
    let mut other = unlock(&dir);
    other.delete(&name(NAME)).unwrap();
    let error = service
        .put_secret(
            &name("new"),
            SecretKind::Generic,
            SecretBytes::from(b"v".to_vec()),
            policy(&[]),
        )
        .unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::Conflict);
    assert!(service.ends_session(&error));
    assert_eq!(service.status().state, VaultState::Locked);
}
