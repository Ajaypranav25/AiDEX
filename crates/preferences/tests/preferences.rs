use aidex_preferences::PreferencesService;
use aidex_vault::{KdfParams, SecretString, VaultService, VaultServiceConfig};
use serde_json::json;
use std::sync::Arc;

#[test]
fn preferences_survive_restart_without_plaintext_private_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(
        VaultService::open(
            dir.path(),
            VaultServiceConfig {
                kdf: KdfParams::new(8, 1, 1).unwrap(),
                idle_timeout: None,
            },
        )
        .unwrap(),
    );
    let password = SecretString::new("correct test passphrase".into());
    vault.init(&password, false).unwrap();
    let prefs = PreferencesService::new(vault.clone());
    prefs.update(json!({"accent":"#abcdef","instructions":"Private user instructions, fixture only","permission_mode":"supervised"})).unwrap();
    let updated = prefs.update(json!({"reduce_motion":true})).unwrap();
    assert_eq!(updated.accent, "#abcdef");
    assert_eq!(
        updated.instructions,
        "Private user instructions, fixture only"
    );
    assert!(updated.reduce_motion);
    let mut folders = vec![dir.path().to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                assert!(
                    !String::from_utf8_lossy(&std::fs::read(path).unwrap())
                        .contains("Private user instructions")
                );
            }
        }
    }
    vault.lock();
    assert!(prefs.get().is_err());
    vault.unlock(&password).unwrap();
    let reopened = PreferencesService::new(vault);
    assert_eq!(reopened.get().unwrap().accent, "#abcdef");
}

#[test]
fn invalid_or_unknown_preference_fields_never_replace_saved_settings() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(
        VaultService::open(
            dir.path(),
            VaultServiceConfig {
                kdf: KdfParams::new(8, 1, 1).unwrap(),
                idle_timeout: None,
            },
        )
        .unwrap(),
    );
    vault
        .init(&SecretString::new("correct test passphrase".into()), false)
        .unwrap();
    let prefs = PreferencesService::new(vault);
    prefs.update(json!({"accent":"#abcdef"})).unwrap();
    assert!(
        prefs
            .update(json!({"api_key":"never persist a credential here"}))
            .is_err()
    );
    assert!(prefs.update(json!({"max_steps":0})).is_err());
    assert!(
        prefs
            .update(json!({"accent":"url(javascript:invalid)"}))
            .is_err()
    );
    assert_eq!(prefs.get().unwrap().accent, "#abcdef");
}
