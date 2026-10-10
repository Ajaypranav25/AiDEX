//! Acceptance: provider/MCP/process consumers receive narrowly scoped handles, and
//! secret-bearing types never serialize or print secret bytes.

mod common;

use aidex_vault::{
    ConsumerScope, HandleDenial, SecretBytes, SecretHandle, SecretKind, SecretString, Vault,
    VaultError, VaultErrorKind, VaultService, VaultServiceConfig,
};
use common::*;
use std::{thread, time::Duration};

const MINUTE: Duration = Duration::from_secs(60);

fn denial(error: VaultError) -> HandleDenial {
    match error {
        VaultError::HandleDenied(reason) => reason,
        other => panic!("expected a handle denial, got {other}"),
    }
}

#[test]
fn allowed_consumers_resolve_and_all_others_are_denied() {
    let root = tempfile::tempdir().unwrap();
    let (_dir, mut vault, _) = create_with_secret(root.path());
    let handle = vault.issue_handle(&name(NAME), &openai(), MINUTE).unwrap();
    assert_eq!(handle.consumer, openai());
    assert_eq!(
        handle.secret_ref,
        vault.meta(&name(NAME)).unwrap().secret_ref
    );
    assert_eq!(
        vault.resolve(&handle, &openai()).unwrap().expose(),
        VALUE.as_bytes()
    );

    for other in [
        github_mcp(),
        ConsumerScope::provider("openai-proxy").unwrap(),
        ConsumerScope::process("git.push").unwrap(),
        ConsumerScope::git("github.com").unwrap(),
    ] {
        assert_eq!(
            denial(vault.resolve(&handle, &other).unwrap_err()),
            HandleDenial::ConsumerMismatch,
            "{other}"
        );
        let error = vault.issue_handle(&name(NAME), &other, MINUTE).unwrap_err();
        assert_eq!(error.kind(), VaultErrorKind::ConsumerNotAllowed, "{other}");
    }
    let error = vault
        .issue_handle(&name("missing"), &openai(), MINUTE)
        .unwrap_err();
    assert_eq!(error.kind(), VaultErrorKind::SecretNotFound);
}

#[test]
fn expired_revoked_deleted_and_repolicied_handles_are_denied() {
    let root = tempfile::tempdir().unwrap();
    let (_dir, mut vault, _) = create_with_secret(root.path());

    let short = vault
        .issue_handle(&name(NAME), &openai(), Duration::from_millis(20))
        .unwrap();
    thread::sleep(Duration::from_millis(60));
    assert_eq!(
        denial(vault.resolve(&short, &openai()).unwrap_err()),
        HandleDenial::Expired
    );

    let revoked = vault.issue_handle(&name(NAME), &openai(), MINUTE).unwrap();
    assert!(vault.revoke_handle(&revoked.handle_id));
    assert_eq!(
        denial(vault.resolve(&revoked, &openai()).unwrap_err()),
        HandleDenial::Revoked
    );

    let a = vault.issue_handle(&name(NAME), &openai(), MINUTE).unwrap();
    let b = vault.issue_handle(&name(NAME), &openai(), MINUTE).unwrap();
    assert_eq!(vault.active_handles(), 2);
    assert_eq!(vault.revoke_all_handles(), 2);
    for handle in [&a, &b] {
        assert_eq!(
            denial(vault.resolve(handle, &openai()).unwrap_err()),
            HandleDenial::Revoked
        );
    }

    let narrowed = vault.issue_handle(&name(NAME), &openai(), MINUTE).unwrap();
    vault
        .put(
            &name(NAME),
            SecretKind::ApiKey,
            SecretBytes::from(VALUE.to_owned()),
            policy(&[github_mcp()]),
        )
        .unwrap();
    assert_eq!(
        denial(vault.resolve(&narrowed, &openai()).unwrap_err()),
        HandleDenial::PolicyRevoked
    );

    let mcp = vault
        .issue_handle(&name(NAME), &github_mcp(), MINUTE)
        .unwrap();
    vault.delete(&name(NAME)).unwrap();
    assert_eq!(
        denial(vault.resolve(&mcp, &github_mcp()).unwrap_err()),
        HandleDenial::SecretDeleted
    );
    // Re-creating a secret with the same name does not revive old handles.
    vault
        .put(
            &name(NAME),
            SecretKind::ApiKey,
            SecretBytes::from(VALUE.to_owned()),
            policy(&[github_mcp()]),
        )
        .unwrap();
    assert_eq!(
        denial(vault.resolve(&mcp, &github_mcp()).unwrap_err()),
        HandleDenial::SecretDeleted
    );
}

#[test]
fn modified_or_foreign_handles_are_denied() {
    let root = tempfile::tempdir().unwrap();
    let (dir, mut vault, _) = create_with_secret(root.path());
    let handle = vault.issue_handle(&name(NAME), &openai(), MINUTE).unwrap();

    let mut retargeted = handle.clone();
    retargeted.consumer = github_mcp();
    assert_eq!(
        denial(vault.resolve(&retargeted, &github_mcp()).unwrap_err()),
        HandleDenial::Mismatch
    );
    let mut extended = handle.clone();
    extended.expires_at = aidex_core::Timestamp::new("9999-01-01T00:00:00Z").unwrap();
    assert_eq!(
        denial(vault.resolve(&extended, &openai()).unwrap_err()),
        HandleDenial::Mismatch
    );

    // Handles belong to one unlock session: locking (or a daemon restart) kills them.
    let locked = vault.lock();
    let reopened = locked.unlock(&passphrase(PASSPHRASE)).unwrap();
    assert_eq!(
        denial(reopened.resolve(&handle, &openai()).unwrap_err()),
        HandleDenial::Unknown
    );
    let other = unlock(&dir);
    assert_eq!(
        denial(other.resolve(&handle, &openai()).unwrap_err()),
        HandleDenial::Unknown
    );
}

#[test]
fn handles_serialize_without_secret_bytes() {
    let root = tempfile::tempdir().unwrap();
    let (_dir, mut vault, _) = create_with_secret(root.path());
    let handle = vault.issue_handle(&name(NAME), &openai(), MINUTE).unwrap();
    let json = serde_json::to_value(&handle).unwrap();
    let keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["consumer", "expires_at", "handle_id", "secret_ref"]);
    let text = json.to_string();
    assert!(!text.contains(VALUE), "{text}");
    assert!(
        !text.contains(NAME),
        "a handle does not even carry the secret name"
    );
    let parsed: SecretHandle = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed, handle);
    assert_eq!(
        vault.resolve(&parsed, &openai()).unwrap().expose(),
        VALUE.as_bytes()
    );
    let meta = serde_json::to_string(&vault.list()).unwrap();
    assert!(!meta.contains(VALUE), "{meta}");
}

#[test]
fn debug_output_of_secret_bearing_types_contains_no_secret() {
    let root = tempfile::tempdir().unwrap();
    let (dir, mut vault, recovery) = create_with_secret(root.path());
    let handle = vault.issue_handle(&name(NAME), &openai(), MINUTE).unwrap();
    let value = vault.resolve(&handle, &openai()).unwrap();
    let recovery_text = recovery.encode();
    let pass = SecretString::from(PASSPHRASE.to_owned());
    let service = VaultService::open(dir.clone(), VaultServiceConfig::default()).unwrap();
    service.unlock(&pass).unwrap();
    let locked = Vault::open(&dir).unwrap();
    let rendered = [
        format!("{value:?} {value}"),
        format!("{pass:?} {pass}"),
        format!("{recovery:?}"),
        format!("{recovery_text:?} {recovery_text}"),
        format!("{vault:?}"),
        format!("{vault:#?}"),
        format!("{locked:?}"),
        format!("{service:?}"),
        format!("{handle:?}"),
        format!("{:?}", service.status()),
        format!("{:?}", vault.list()),
    ];
    for text in rendered {
        for secret in [VALUE, PASSPHRASE, recovery_text.expose()] {
            assert!(!text.contains(secret), "{text}");
        }
    }
}
