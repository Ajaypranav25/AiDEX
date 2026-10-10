//! Explicit, retryable schema v1 → v2 manifest migration.
//!
//! Procedure (each step is safe to repeat after an interruption):
//! 1. Transform and validate the v1 layout in memory; on failure nothing is written.
//! 2. Preserve the original manifest bytes in [`V1_BACKUP_FILE_NAME`] with create-new
//!    semantics. An existing backup with identical bytes is accepted (a previous attempt
//!    got this far); different content aborts with [`DriveError::MigrationConflict`].
//! 3. Write the v2 manifest to a temporary root-level file and fsync it.
//! 4. Confirm the live manifest still holds the original bytes, then atomically rename
//!    the temporary file over `aidex-drive.json`.
//! 5. Re-read and verify the installed manifest. The backup is always kept.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::Path,
};

use crate::{
    DriveError, DriveManifest, DriveManifestV1, MANIFEST_FILE_NAME, StoredManifest,
    ops::read_stored_manifest,
};

/// Root-level file holding the untouched bytes of a migrated schema-v1 manifest.
pub const V1_BACKUP_FILE_NAME: &str = "aidex-drive.v1.json";

/// Root-level staging file for the transformed manifest; stale copies are replaced.
pub(crate) const MIGRATION_TEMP_FILE_NAME: &str = "aidex-drive.json.v2-tmp";

/// Migrate the v1 manifest at `root`, whose on-disk bytes are `original`.
pub(crate) fn migrate_v1_manifest(
    root: &Path,
    original: &[u8],
    legacy: &DriveManifestV1,
) -> Result<DriveManifest, DriveError> {
    let manifest_path = root.join(MANIFEST_FILE_NAME);
    let unchanged = |reason: String| {
        DriveError::MigrationFailed(format!(
            "{reason}; the schema v1 manifest `{}` was left unchanged",
            manifest_path.display()
        ))
    };
    let migrated = legacy.migrate_to_v2().map_err(|error| {
        unchanged(format!(
            "the v1 layout cannot become a valid v2 layout ({error})"
        ))
    })?;
    let json = migrated
        .to_json()
        .map_err(|error| unchanged(format!("cannot encode the v2 manifest ({error})")))?;

    preserve_original(root, original)?;

    let temp_path = root.join(MIGRATION_TEMP_FILE_NAME);
    write_staged(&temp_path, json.as_bytes())?;
    let live = fs::read(&manifest_path).map_err(|error| {
        DriveError::Io(format!(
            "cannot re-read manifest `{}` before migration: {error}",
            manifest_path.display()
        ))
    })?;
    if live != original {
        remove_staged(&temp_path);
        return Err(unchanged(
            "the manifest changed while migration was in progress".into(),
        ));
    }
    fs::rename(&temp_path, &manifest_path).map_err(|error| {
        DriveError::MigrationFailed(format!(
            "cannot install the v2 manifest over `{}` ({error}); the original is preserved in `{V1_BACKUP_FILE_NAME}` and `init` can be retried",
            manifest_path.display()
        ))
    })?;
    sync_directory(root);

    match read_stored_manifest(&manifest_path) {
        Ok((StoredManifest::V2(installed), _)) if installed == migrated => Ok(installed),
        Ok(_) => Err(DriveError::MigrationFailed(format!(
            "the installed manifest `{}` does not match the migrated layout; the original is preserved in `{V1_BACKUP_FILE_NAME}`",
            manifest_path.display()
        ))),
        Err(error) => Err(DriveError::MigrationFailed(format!(
            "cannot verify the installed manifest ({error}); the original is preserved in `{V1_BACKUP_FILE_NAME}`"
        ))),
    }
}

/// Write the original manifest bytes to the backup path without replacing anything.
fn preserve_original(root: &Path, original: &[u8]) -> Result<(), DriveError> {
    let backup = root.join(V1_BACKUP_FILE_NAME);
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
    {
        Ok(mut file) => {
            if let Err(error) = file.write_all(original).and_then(|()| file.sync_all()) {
                drop(file);
                // Never leave a partial backup that would block a retry.
                let _ = fs::remove_file(&backup);
                return Err(DriveError::Io(format!(
                    "cannot write manifest backup `{}`: {error}; the schema v1 manifest was left unchanged",
                    backup.display()
                )));
            }
            sync_directory(root);
            Ok(())
        }
        // Windows reports access-denied rather than already-exists for a directory.
        Err(error)
            if error.kind() == ErrorKind::AlreadyExists
                || fs::symlink_metadata(&backup).is_ok() =>
        {
            let regular = fs::symlink_metadata(&backup)
                .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink());
            if regular && fs::read(&backup).is_ok_and(|existing| existing == original) {
                Ok(())
            } else {
                Err(DriveError::MigrationConflict(backup))
            }
        }
        Err(error) => Err(DriveError::Io(format!(
            "cannot create manifest backup `{}`: {error}; the schema v1 manifest was left unchanged",
            backup.display()
        ))),
    }
}

/// Create the staging file fresh, replacing only a stale regular file from an earlier attempt.
fn write_staged(path: &Path, bytes: &[u8]) -> Result<(), DriveError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::remove_file(path).map_err(|error| {
                DriveError::Io(format!(
                    "cannot remove stale migration file `{}`: {error}",
                    path.display()
                ))
            })?;
        }
        Ok(_) => return Err(DriveError::UnsafeComponent(path.to_path_buf())),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(DriveError::Io(format!(
                "cannot inspect migration file `{}`: {error}",
                path.display()
            )));
        }
    }
    let result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()));
    result.map_err(|error| {
        remove_staged(path);
        DriveError::Io(format!(
            "cannot write migration file `{}`: {error}; the schema v1 manifest was left unchanged",
            path.display()
        ))
    })
}

fn remove_staged(path: &Path) {
    // Best effort: a leftover staging file is replaced on the next attempt.
    let _ = fs::remove_file(path);
}

/// Persist directory entries (create, rename) where the platform supports it.
fn sync_directory(root: &Path) {
    #[cfg(unix)]
    {
        // Best effort: some removable filesystems reject directory fsync.
        let _ = fs::File::open(root).and_then(|directory| directory.sync_all());
    }
    #[cfg(not(unix))]
    let _ = root;
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
