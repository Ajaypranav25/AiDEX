//! Shared fixtures for vault integration tests. Values are obviously fake markers.
#![allow(dead_code)]

use aidex_vault::{
    ConsumerScope, CreateOptions, KdfParams, RecoveryKey, SecretBytes, SecretKind, SecretName,
    SecretPolicy, SecretString, UnlockedVault, Vault,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const PASSPHRASE: &str = "correct horse battery staple";
pub const VALUE: &str = "PLAINTEXT-MARKER-VALUE-7d1f2c";
pub const NAME: &str = "marker-secret-name";

/// Tiny Argon2id cost so debug-build tests stay fast. Never use outside tests.
pub fn tiny() -> KdfParams {
    KdfParams::new(8, 1, 1).unwrap()
}

pub fn passphrase(text: &str) -> SecretString {
    SecretString::from(text.to_owned())
}

pub fn name(text: &str) -> SecretName {
    SecretName::new(text).unwrap()
}

pub fn openai() -> ConsumerScope {
    ConsumerScope::provider("openai").unwrap()
}

pub fn github_mcp() -> ConsumerScope {
    ConsumerScope::mcp("github").unwrap()
}

pub fn policy(consumers: &[ConsumerScope]) -> SecretPolicy {
    SecretPolicy::new(consumers.iter().cloned()).unwrap()
}

pub fn vault_dir(root: &Path) -> PathBuf {
    root.join("private_vault")
}

/// A new vault (with a recovery key) holding [`NAME`] = [`VALUE`] for `provider:openai`.
pub fn create_with_secret(root: &Path) -> (PathBuf, UnlockedVault, RecoveryKey) {
    let dir = vault_dir(root);
    let (mut vault, recovery) = Vault::create(
        &dir,
        &passphrase(PASSPHRASE),
        CreateOptions {
            kdf: tiny(),
            recovery_key: true,
        },
    )
    .unwrap();
    vault
        .put(
            &name(NAME),
            SecretKind::ApiKey,
            SecretBytes::from(VALUE.to_owned()),
            policy(&[openai()]),
        )
        .unwrap();
    (dir, vault, recovery.expect("recovery key requested"))
}

/// Resolve `name` for `provider:openai` through a fresh short-lived handle.
pub fn reveal(vault: &mut UnlockedVault, secret: &str) -> Vec<u8> {
    let handle = vault
        .issue_handle(&name(secret), &openai(), std::time::Duration::from_secs(60))
        .unwrap();
    vault.resolve(&handle, &openai()).unwrap().expose().to_vec()
}

pub fn unlock(dir: &Path) -> UnlockedVault {
    Vault::open(dir)
        .unwrap()
        .unlock(&passphrase(PASSPHRASE))
        .unwrap()
}

pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Every regular file beneath `dir`.
pub fn files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            found.extend(files(&entry.path()));
        } else {
            found.push(entry.path());
        }
    }
    found.sort();
    found
}

pub fn record_files(dir: &Path) -> Vec<PathBuf> {
    files(&dir.join("records"))
}

pub fn header_path(dir: &Path) -> PathBuf {
    dir.join("vault.json")
}

/// Edit `vault.json` as JSON and write it back.
pub fn edit_header(dir: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = header_path(dir);
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut value);
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

/// Replace one base64 character in `text` so it decodes to different bytes.
pub fn mutate_base64(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    chars[0] = if chars[0] == 'A' { 'B' } else { 'A' };
    chars.into_iter().collect()
}

/// Assert no file beneath `dir` contains `needle`.
pub fn assert_absent(dir: &Path, needle: &[u8]) {
    for file in files(dir) {
        let bytes = fs::read(&file).unwrap();
        assert!(
            !bytes.windows(needle.len()).any(|window| window == needle),
            "{} contains {:?}",
            file.display(),
            String::from_utf8_lossy(needle)
        );
        assert!(
            !file
                .to_string_lossy()
                .contains(&*String::from_utf8_lossy(needle)),
            "file name {} contains the needle",
            file.display()
        );
    }
}
