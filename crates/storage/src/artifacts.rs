use std::{
    fs, io,
    sync::atomic::{AtomicU64, Ordering},
};

use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::{StoreError, paths::reject_symlink_if_exists, store::FileStore};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl FileStore {
    /// Persist bytes once by SHA-256 and return the lowercase hexadecimal content hash.
    pub fn put_artifact(&self, bytes: &[u8]) -> Result<String, StoreError> {
        let hash = hex_hash(bytes);
        let destination = self.artifacts.join(&hash);
        reject_symlink_if_exists(&destination)?;
        if destination.exists() {
            verify_blob(&destination, &hash)?;
        } else {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary = self
                .artifacts
                .join(format!(".tmp-{}-{sequence}", std::process::id()));
            let result = write_atomic_blob(&temporary, &destination, bytes);
            if let Err(error) = result {
                let _ = fs::remove_file(&temporary);
                if !destination.exists() {
                    return Err(error);
                }
                verify_blob(&destination, &hash)?;
            }
        }
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR IGNORE INTO artifacts(hash, size) VALUES (?1, ?2)",
            params![hash, bytes.len() as i64],
        )?;
        Ok(hash)
    }

    /// Read and verify an artifact. Missing metadata or content is reported as corruption.
    pub fn get_artifact(&self, hash: &str) -> Result<Vec<u8>, StoreError> {
        if !valid_hash(hash) {
            return Err(StoreError::InvalidKey);
        }
        let connection = self.connection()?;
        let size: Option<i64> = connection
            .query_row(
                "SELECT size FROM artifacts WHERE hash = ?1",
                [hash],
                |row| row.get(0),
            )
            .optional()?;
        let Some(size) = size else {
            return Err(StoreError::Corrupt(format!(
                "artifact metadata missing for {hash}"
            )));
        };

        let path = self.artifacts.join(hash);
        reject_symlink_if_exists(&path)?;
        let bytes = fs::read(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                StoreError::Corrupt(format!("artifact file missing for {hash}"))
            } else {
                StoreError::Io(error)
            }
        })?;
        if bytes.len() as i64 != size || hex_hash(&bytes) != hash {
            return Err(StoreError::Corrupt(format!(
                "artifact hash/size mismatch for {hash}"
            )));
        }
        Ok(bytes)
    }
}

fn write_atomic_blob(
    temporary: &std::path::Path,
    destination: &std::path::Path,
    bytes: &[u8],
) -> Result<(), StoreError> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, destination)?;
    Ok(())
}

fn verify_blob(path: &std::path::Path, expected_hash: &str) -> Result<(), StoreError> {
    reject_symlink_if_exists(path)?;
    let bytes = fs::read(path)?;
    if hex_hash(&bytes) != expected_hash {
        return Err(StoreError::Corrupt(format!(
            "artifact content mismatch for {expected_hash}"
        )));
    }
    Ok(())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn hex_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
