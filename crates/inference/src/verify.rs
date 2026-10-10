//! Model verification before use (spec §86).
//!
//! Policy: a model is fully SHA-256 hashed before its first load and whenever
//! `force` is set (`aidex model verify`). A successful full hash is cached in
//! `cache/verified-models.json` keyed by (drive-relative path, size, modification
//! time, expected SHA-256); later loads reuse it while all four still match, so
//! a multi-gigabyte model is not rehashed on every launch. Any change of size or
//! modification time, or a different recorded hash, forces a full rehash. A
//! mismatch refuses the model and drops its cache record. The cache trusts file
//! metadata, so a modification that preserves size and timestamp is caught only
//! by a forced verification.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use aidex_core::{CancelToken, Timestamp};
use aidex_drive::DrivePaths;
use serde::{Deserialize, Serialize};

use crate::{
    InferenceError, LocalModelId, RegistryEntry, Result,
    fsutil::{self, display},
};

/// Verification cache file inside `DrivePaths::cache()`.
pub const VERIFIED_CACHE_FILE: &str = "verified-models.json";
const CACHE_VERSION: u32 = 1;
const MAX_CACHE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CACHE_RECORDS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    /// The whole file was hashed now.
    FullHash,
    /// An earlier full hash of the unchanged file was reused.
    Cached,
}

/// A model that passed verification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verified {
    pub id: LocalModelId,
    pub sha256: String,
    pub size_bytes: u64,
    pub method: VerificationMethod,
    /// When the full hash that this result rests on was computed.
    pub hashed_at: Timestamp,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheRecord {
    file: String,
    size_bytes: u64,
    modified_unix_nanos: String,
    sha256: String,
    hashed_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheFile {
    version: u32,
    records: Vec<CacheRecord>,
}

/// The verification cache of one drive.
#[derive(Clone, Debug)]
pub struct VerificationCache {
    path: PathBuf,
}

impl VerificationCache {
    pub fn open(paths: &DrivePaths) -> Result<Self> {
        let dir = paths.ensure_dir(&paths.cache())?;
        Ok(Self {
            path: dir.join(VERIFIED_CACHE_FILE),
        })
    }

    fn load(&self) -> Vec<CacheRecord> {
        // An unreadable cache is discarded: it only ever causes a rehash.
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return Vec::new();
        };
        if !metadata.is_file() || metadata.len() > MAX_CACHE_BYTES {
            return Vec::new();
        }
        fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CacheFile>(&bytes).ok())
            .filter(|cache| cache.version == CACHE_VERSION)
            .map(|cache| cache.records)
            .unwrap_or_default()
    }

    fn store(&self, mut records: Vec<CacheRecord>) -> Result<()> {
        if records.len() > MAX_CACHE_RECORDS {
            let excess = records.len() - MAX_CACHE_RECORDS;
            records.drain(..excess);
        }
        let json = serde_json::to_vec_pretty(&CacheFile {
            version: CACHE_VERSION,
            records,
        })
        .map_err(|error| InferenceError::Io {
            context: "cannot encode the verification cache".into(),
            message: error.to_string(),
        })?;
        fsutil::write_atomic(&self.path, &json)
    }

    /// Record a full hash of `file` taken just now.
    pub(crate) fn record(&self, file: &str, path: &Path, sha256: &str) -> Result<()> {
        let metadata = fsutil::regular_file(path, "model file")?;
        let mut records = self.load();
        records.retain(|record| record.file != file);
        records.push(CacheRecord {
            file: file.to_owned(),
            size_bytes: metadata.len(),
            modified_unix_nanos: fsutil::modified_nanos(&metadata).to_string(),
            sha256: sha256.to_owned(),
            hashed_at: Timestamp::now(),
        });
        self.store(records)
    }

    /// Drop the record for `file`.
    pub(crate) fn forget(&self, file: &str) -> Result<()> {
        let mut records = self.load();
        let before = records.len();
        records.retain(|record| record.file != file);
        if records.len() != before {
            self.store(records)?;
        }
        Ok(())
    }

    fn lookup(&self, file: &str, metadata: &fs::Metadata, sha256: &str) -> Option<Timestamp> {
        let modified = fsutil::modified_nanos(metadata).to_string();
        self.load()
            .into_iter()
            .find(|record| {
                record.file == file
                    && record.size_bytes == metadata.len()
                    && record.modified_unix_nanos == modified
                    && record.sha256.eq_ignore_ascii_case(sha256)
            })
            .map(|record| record.hashed_at)
    }
}

/// Verify `entry` (see the module policy).
pub fn verify_model(paths: &DrivePaths, entry: &RegistryEntry) -> Result<Verified> {
    verify_model_with(paths, entry, false, &mut |_| {}, &CancelToken::new())
}

/// Verify `entry`; `force` skips the cache. `progress` receives hashed bytes.
pub fn verify_model_with(
    paths: &DrivePaths,
    entry: &RegistryEntry,
    force: bool,
    progress: &mut dyn FnMut(u64),
    cancel: &CancelToken,
) -> Result<Verified> {
    let started = Instant::now();
    let path = entry.model_path(paths)?;
    let metadata = fsutil::regular_file(&path, &format!("model file of `{}`", entry.id))?;
    let cache = VerificationCache::open(paths)?;
    let elapsed = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if metadata.len() != entry.size_bytes {
        let _ = cache.forget(&entry.file);
        return Err(InferenceError::HashMismatch {
            what: format!("model `{}` ({})", entry.id, display(&path)),
            expected: format!("{} ({} bytes)", entry.sha256, entry.size_bytes),
            actual: format!("a file of {} bytes", metadata.len()),
        });
    }
    if !force {
        if let Some(hashed_at) = cache.lookup(&entry.file, &metadata, &entry.sha256) {
            return Ok(Verified {
                id: entry.id.clone(),
                sha256: entry.sha256.clone(),
                size_bytes: entry.size_bytes,
                method: VerificationMethod::Cached,
                hashed_at,
                duration_ms: elapsed(),
            });
        }
    }
    let (digest, _) = fsutil::sha256_file(&path, progress, cancel)?;
    if !digest.eq_ignore_ascii_case(&entry.sha256) {
        let _ = cache.forget(&entry.file);
        return Err(InferenceError::HashMismatch {
            what: format!("model `{}` ({})", entry.id, display(&path)),
            expected: entry.sha256.clone(),
            actual: digest,
        });
    }
    cache.record(&entry.file, &path, &digest)?;
    Ok(Verified {
        id: entry.id.clone(),
        sha256: digest,
        size_bytes: entry.size_bytes,
        method: VerificationMethod::FullHash,
        hashed_at: Timestamp::now(),
        duration_ms: elapsed(),
    })
}
