//! Encrypted artifact blobs beside the database (`<db stem>-artifacts/`).
//!
//! A blob file holds sealed segments only; its name is the SHA-256 of those
//! sealed bytes, so names reveal nothing about the plaintext. Files are written
//! to a fresh temporary name, synced, and renamed into place before the row that
//! references them commits; unreferenced files are swept when the store opens.
use crate::StoreError;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub(crate) struct BlobDir {
    root: PathBuf,
}

impl BlobDir {
    pub(crate) fn beside(database: &Path) -> Self {
        let stem = database
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "native".into());
        Self {
            root: database.with_file_name(format!("{stem}-artifacts")),
        }
    }

    fn check_root(&self) -> Result<(), StoreError> {
        if fs::symlink_metadata(&self.root).is_ok_and(|m| !m.is_dir()) {
            return Err("artifact blob directory is not a plain directory".into());
        }
        Ok(())
    }

    fn path(&self, address: &str) -> Result<PathBuf, StoreError> {
        if !valid_address(address) {
            // Not "invalid artifact ...": that prefix marks caller mistakes.
            return Err("malformed encrypted artifact blob address".into());
        }
        Ok(self.root.join(address))
    }

    /// Store sealed bytes and return their content address.
    pub(crate) fn write(&self, sealed: &[u8]) -> Result<String, StoreError> {
        self.check_root()?;
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        self.check_root()?;
        let address = format!("{:x}", Sha256::digest(sealed));
        let target = self.path(&address)?;
        let mut suffix = [0u8; 8];
        getrandom::fill(&mut suffix).map_err(|e| e.to_string())?;
        let temporary = self.root.join(format!(
            "{}.tmp",
            suffix
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ));
        let written = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(sealed)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &target)
        })();
        if let Err(error) = written {
            let _ = fs::remove_file(&temporary);
            return Err(format!("artifact blob write failed: {error}"));
        }
        Ok(address)
    }

    /// Read `len` sealed bytes at `start` from a blob of `expected` sealed bytes.
    pub(crate) fn read(
        &self,
        address: &str,
        expected: u64,
        start: u64,
        len: u64,
    ) -> Result<Vec<u8>, StoreError> {
        self.check_root()?;
        let path = self.path(address)?;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| "encrypted artifact blob is missing".to_string())?;
        if !metadata.is_file() {
            return Err("encrypted artifact blob is not a regular file".into());
        }
        if metadata.len() != expected {
            return Err("truncated encrypted artifact".into());
        }
        let mut file = File::open(&path).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(start))
            .map_err(|e| e.to_string())?;
        let mut out = vec![0u8; len as usize];
        file.read_exact(&mut out)
            .map_err(|_| "truncated encrypted artifact".to_string())?;
        Ok(out)
    }

    /// Remove a blob after the row that referenced it is gone (best effort; a
    /// leftover file is swept on the next open).
    pub(crate) fn remove(&self, address: &str) {
        if self.check_root().is_ok()
            && let Ok(path) = self.path(address)
        {
            let _ = fs::remove_file(path);
        }
    }

    /// Delete temporary files and blobs no row references.
    pub(crate) fn sweep(&self, referenced: impl Fn(&str) -> Result<bool, StoreError>) {
        if self.check_root().is_err() {
            return;
        }
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_file = entry.file_type().is_ok_and(|t| t.is_file());
            let orphan = if name.ends_with(".tmp") {
                true
            } else {
                valid_address(&name) && referenced(&name).is_ok_and(|used| !used)
            };
            if is_file && orphan {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

fn valid_address(address: &str) -> bool {
    address.len() == 64
        && address
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blobs_are_content_addressed_and_confined() {
        let dir = tempfile::tempdir().unwrap();
        let blobs = BlobDir::beside(&dir.path().join("native.sqlite3"));
        assert_eq!(blobs.root(), dir.path().join("native-artifacts"));
        let address = blobs.write(b"sealed fixture bytes").unwrap();
        assert_eq!(
            address,
            format!("{:x}", Sha256::digest(b"sealed fixture bytes"))
        );
        assert_eq!(blobs.read(&address, 20, 7, 7).unwrap(), b"fixture");
        assert!(blobs.read(&address, 21, 0, 1).is_err());
        assert!(blobs.read("../native.sqlite3", 20, 0, 1).is_err());
        std::fs::write(blobs.root().join("stale.tmp"), b"x").unwrap();
        blobs.sweep(|_| Ok(true));
        assert!(!blobs.root().join("stale.tmp").exists());
        assert!(blobs.root().join(&address).exists());
        blobs.sweep(|_| Err("database unavailable".into()));
        assert!(blobs.root().join(&address).exists());
        blobs.sweep(|_| Ok(false));
        assert!(!blobs.root().join(&address).exists());
    }
}
