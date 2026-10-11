//! `app/update.lock`: one update or rollback per drive at a time.
//!
//! The lock is a file created exclusively (`create_new`) that records the holder. It
//! is removed when the operation finishes. A crash leaves it behind; the daemon is
//! the drive's only updater and a per-drive singleton, so [`crate::Updater::recover`]
//! (which runs before the daemon serves requests) removes a leftover lock.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use aidex_core::{EntityId, Timestamp};

use crate::UpdateError;

/// File name of the lock inside the drive's `app/` directory.
pub const LOCK_FILE_NAME: &str = "update.lock";

/// A held update lock.
#[derive(Debug)]
pub(crate) struct DriveLock {
    path: PathBuf,
}

impl DriveLock {
    pub(crate) fn acquire(app: &Path, id: &EntityId) -> Result<Self, UpdateError> {
        let path = app.join(LOCK_FILE_NAME);
        let holder = format!(
            "update {} by process {} since {}",
            id.as_str(),
            std::process::id(),
            Timestamp::now().as_str()
        );
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(holder.as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(|error| {
                        UpdateError::io(format!("cannot write `{}`", path.display()), error)
                    })?;
                Ok(Self { path })
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => Err(UpdateError::Busy {
                holder: read_holder(&path),
            }),
            Err(error) => Err(UpdateError::io(
                format!("cannot create `{}`", path.display()),
                error,
            )),
        }
    }

    /// Remove the lock (the operation finished, successfully or not).
    pub(crate) fn release(self) {
        let _ = fs::remove_file(&self.path);
    }

    /// Leave the lock in place, as a crash would (fault injection).
    pub(crate) fn abandon(self) {}
}

/// Remove a lock left behind by a crashed operation. Returns its holder text.
pub(crate) fn remove_stale(app: &Path) -> Result<Option<String>, UpdateError> {
    let path = app.join(LOCK_FILE_NAME);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(UpdateError::io(
            format!("cannot inspect `{}`", path.display()),
            error,
        )),
        Ok(metadata) => {
            let holder = if metadata.is_file() {
                read_holder(&path)
            } else {
                "not a regular file".to_owned()
            };
            let removed = if metadata.is_dir() {
                fs::remove_dir(&path)
            } else {
                fs::remove_file(&path)
            };
            removed.map_err(|error| {
                UpdateError::io(format!("cannot remove `{}`", path.display()), error)
            })?;
            Ok(Some(holder))
        }
    }
}

fn read_holder(path: &Path) -> String {
    let mut text = String::new();
    let _ = fs::File::open(path).and_then(|file| file.take(512).read_to_string(&mut text));
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    if text.is_empty() {
        format!("`{}` exists", path.display())
    } else {
        text
    }
}
