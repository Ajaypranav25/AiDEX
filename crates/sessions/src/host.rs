//! Privacy-preserving host fingerprints (spec §62).
//!
//! A fingerprint is `SHA-256("aidex.host-fingerprint.v1" ‖ drive salt ‖ os ‖ arch ‖
//! hostname)`, truncated to 128 bits and hex encoded. The 32-byte salt is random,
//! created once per drive at `<private_config>/host-salt`, so the same machine has
//! unrelated fingerprints on different drives and a fingerprint cannot be matched
//! against a hostname without the drive. Only the operating system and architecture
//! are kept in clear (they are needed to explain a cross-host resume); the hostname
//! never leaves this function. No hardware identifiers are read.

use std::{
    fs,
    io::{ErrorKind, Write},
    path::Path,
};

use aidex_drive::DrivePaths;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::SessionError;

/// Salt file inside the drive's private config directory.
pub const HOST_SALT_FILE: &str = "host-salt";
/// Salt length in bytes.
pub const HOST_SALT_BYTES: usize = 32;
/// Hex characters in [`HostFingerprint::id`] (128 bits).
pub const FINGERPRINT_HEX_LEN: usize = 32;

const DOMAIN: &[u8] = b"aidex.host-fingerprint.v1";
const MAX_PLATFORM_LEN: usize = 32;

/// Who a session last ran on: an unlinkable id plus the platform in clear.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostFingerprint {
    /// 32 lowercase hex characters.
    pub id: String,
    /// `std::env::consts::OS`, e.g. `windows`, `linux`, `macos`.
    pub os: String,
    /// `std::env::consts::ARCH`, e.g. `x86_64`, `aarch64`.
    pub arch: String,
}

impl HostFingerprint {
    /// Fingerprint explicit host facts (deterministic; for tests and other hosts).
    pub fn compute(salt: &[u8], os: &str, arch: &str, hostname: &str) -> Self {
        let mut hasher = Sha256::new();
        for part in [
            DOMAIN,
            salt,
            os.as_bytes(),
            arch.as_bytes(),
            normalize_hostname(hostname).as_bytes(),
        ] {
            hasher.update((part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        let digest = hasher.finalize();
        let id = digest[..FINGERPRINT_HEX_LEN / 2]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Self {
            id,
            os: os.to_owned(),
            arch: arch.to_owned(),
        }
    }

    /// Fingerprint the running host with `salt`.
    pub fn current(salt: &[u8]) -> Self {
        let hostname = gethostname::gethostname();
        Self::compute(
            salt,
            std::env::consts::OS,
            std::env::consts::ARCH,
            &hostname.to_string_lossy(),
        )
    }

    /// Fingerprint the running host with the drive's salt (created on first use).
    pub fn for_drive(paths: &DrivePaths) -> Result<Self, SessionError> {
        Ok(Self::current(&load_or_create_host_salt(paths)?))
    }

    /// Reject a malformed fingerprint (e.g. from a request).
    pub fn validate(&self) -> Result<(), SessionError> {
        let platform = |value: &str| {
            !value.is_empty()
                && value.len() <= MAX_PLATFORM_LEN
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        };
        if self.id.len() != FINGERPRINT_HEX_LEN
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !platform(&self.os)
            || !platform(&self.arch)
        {
            return Err(SessionError::Corrupt("malformed host fingerprint".into()));
        }
        Ok(())
    }

    /// Human-readable differences from `previous`; empty when it is the same host.
    pub fn changes_from(&self, previous: &Self) -> Vec<String> {
        let mut changes = Vec::new();
        if self.os != previous.os {
            changes.push(format!(
                "operating system changed from {} to {}",
                previous.os, self.os
            ));
        }
        if self.arch != previous.arch {
            changes.push(format!(
                "architecture changed from {} to {}",
                previous.arch, self.arch
            ));
        }
        if self.id != previous.id && changes.is_empty() {
            changes.push(
                "different machine (host fingerprint changed; host-bound state such as processes, \
                 local runtimes, and model availability must be re-detected)"
                    .into(),
            );
        }
        changes
    }
}

fn normalize_hostname(hostname: &str) -> String {
    hostname.trim().to_ascii_lowercase()
}

/// Read the drive's host salt, creating it once from the OS RNG.
///
/// A salt that exists but is not exactly [`HOST_SALT_BYTES`] bytes is reported as
/// corrupt rather than replaced (replacing it would silently make every host look
/// new); a symlinked or non-regular salt path is refused.
pub fn load_or_create_host_salt(paths: &DrivePaths) -> Result<[u8; HOST_SALT_BYTES], SessionError> {
    let config = paths.ensure_dir(&paths.config())?;
    let path = config.join(HOST_SALT_FILE);
    if let Some(salt) = read_salt(&path)? {
        return Ok(salt);
    }
    let mut salt = [0_u8; HOST_SALT_BYTES];
    getrandom::fill(&mut salt).map_err(|error| {
        SessionError::Unavailable(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).map_err(|error| {
        SessionError::Unavailable(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    let temporary = config.join(format!(
        ".{HOST_SALT_FILE}.tmp-{}-{:016x}",
        std::process::id(),
        u64::from_le_bytes(nonce)
    ));
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&salt)?;
        file.sync_all()
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    // Another daemon may have won the race; keep the salt that is already there.
    if let Some(existing) = read_salt(&path)? {
        let _ = fs::remove_file(&temporary);
        return Ok(existing);
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    sync_dir(&config);
    Ok(salt)
}

fn read_salt(path: &Path) -> Result<Option<[u8; HOST_SALT_BYTES]>, SessionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(SessionError::UnsafePath(path.to_path_buf()))
        }
        Ok(_) => {
            let bytes = fs::read(path)?;
            <[u8; HOST_SALT_BYTES]>::try_from(bytes.as_slice())
                .map(Some)
                .map_err(|_| {
                    SessionError::Corrupt(format!(
                        "the drive host salt is {} bytes; expected {HOST_SALT_BYTES} (restore it from a backup or remove it to start a new host history)",
                        bytes.len()
                    ))
                })
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Persist the rename on filesystems that need a directory sync (best effort).
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_depend_on_salt_and_host_but_not_hostname_case() {
        let salt = [7_u8; HOST_SALT_BYTES];
        let a = HostFingerprint::compute(&salt, "windows", "x86_64", "Laptop-A");
        assert_eq!(a.id.len(), FINGERPRINT_HEX_LEN);
        a.validate().unwrap();
        assert_eq!(
            a,
            HostFingerprint::compute(&salt, "windows", "x86_64", " laptop-a ")
        );
        assert_ne!(
            a.id,
            HostFingerprint::compute(&salt, "windows", "x86_64", "laptop-b").id
        );
        assert_ne!(
            a.id,
            HostFingerprint::compute(&[8_u8; HOST_SALT_BYTES], "windows", "x86_64", "laptop-a").id,
            "another drive's salt makes the same host unlinkable"
        );
        assert!(!a.id.contains("laptop"));
    }

    #[test]
    fn changes_name_platform_differences_or_a_different_machine() {
        let salt = [1_u8; HOST_SALT_BYTES];
        let laptop = HostFingerprint::compute(&salt, "windows", "x86_64", "laptop");
        assert!(laptop.changes_from(&laptop).is_empty());
        let workstation = HostFingerprint::compute(&salt, "windows", "x86_64", "workstation");
        let changes = workstation.changes_from(&laptop);
        assert_eq!(changes.len(), 1);
        assert!(changes[0].contains("different machine"));
        let mac = HostFingerprint::compute(&salt, "macos", "aarch64", "laptop");
        let changes = mac.changes_from(&laptop);
        assert_eq!(changes.len(), 2);
        assert!(changes[0].contains("windows to macos"));
        assert!(changes[1].contains("x86_64 to aarch64"));
    }

    #[test]
    fn malformed_fingerprints_are_rejected() {
        for (id, os) in [
            ("abc", "linux"),
            (&"G".repeat(32) as &str, "linux"),
            (&"a".repeat(32), "Linux OS"),
        ] {
            let fingerprint = HostFingerprint {
                id: id.into(),
                os: os.into(),
                arch: "x86_64".into(),
            };
            assert!(fingerprint.validate().is_err(), "{id} {os}");
        }
    }
}
