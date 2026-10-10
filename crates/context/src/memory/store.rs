//! The drive-backed memory store: one human-editable TOML file per scope.
//!
//! ```text
//! private_memory/user.toml                    portable user memory
//! private_memory/workspaces/<workspace>.toml  workspace memory
//! private_memory/sessions/<session>.toml      session memory
//! ```

use std::{
    path::PathBuf,
    sync::Mutex,
};

use aidex_core::{EntityId, Timestamp};
use aidex_drive::DrivePaths;
use serde::{Deserialize, Serialize};

use super::model::{
    MAX_ENTRIES_PER_SCOPE, MEMORY_ID_PREFIX, MemoryEdit, MemoryEntry, MemoryError, MemoryScope,
    MemoryStore, NewMemory, validate_tags, validate_text,
};
use crate::{error::file_safe_id, fsio};

/// Memory file format version.
pub const MEMORY_SCHEMA_VERSION: i64 = 1;
/// Largest memory file read.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const USER_FILE: &str = "user.toml";
const WORKSPACE_DIR: &str = "workspaces";
const SESSION_DIR: &str = "sessions";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryFile {
    schema_version: i64,
    #[serde(default)]
    entries: Vec<MemoryEntry>,
}

/// [`MemoryStore`] over TOML files in `DrivePaths::memory()`. Writes are
/// serialized in-process and replace files atomically; symlinked files and
/// directories are refused.
pub struct FileMemoryStore {
    paths: DrivePaths,
    lock: Mutex<()>,
}

impl FileMemoryStore {
    pub fn open(paths: &DrivePaths) -> Result<Self, MemoryError> {
        paths
            .ensure_dir(&paths.memory())
            .map_err(|error| MemoryError::Io(error.to_string()))?;
        Ok(Self {
            paths: paths.clone(),
            lock: Mutex::new(()),
        })
    }

    /// The file holding `scope` (its directory is created on demand).
    pub fn file_for(&self, scope: &MemoryScope) -> Result<PathBuf, MemoryError> {
        let base = self.paths.memory();
        let (dir, name) = match scope {
            MemoryScope::User => (base, USER_FILE.to_owned()),
            MemoryScope::Workspace { workspace_id } => (
                base.join(WORKSPACE_DIR),
                format!("{}.toml", safe(workspace_id)?),
            ),
            MemoryScope::Session { session_id } => (
                base.join(SESSION_DIR),
                format!("{}.toml", safe(session_id)?),
            ),
        };
        let dir = self
            .paths
            .ensure_dir(&dir)
            .map_err(|error| MemoryError::Io(error.to_string()))?;
        Ok(dir.join(name))
    }

    fn load(&self, scope: &MemoryScope) -> Result<(PathBuf, Vec<MemoryEntry>), MemoryError> {
        let path = self.file_for(scope)?;
        let shown = path.display().to_string();
        let bytes = fsio::read_bounded(&path, MAX_FILE_BYTES)
            .map_err(|error| MemoryError::Io(format!("{shown}: {error}")))?;
        let Some(bytes) = bytes else {
            return Ok((path, Vec::new()));
        };
        let text = String::from_utf8(bytes).map_err(|_| MemoryError::Corrupt {
            file: shown.clone(),
            message: "not UTF-8".into(),
        })?;
        let file: MemoryFile = toml::from_str(&text).map_err(|error| MemoryError::Corrupt {
            file: shown.clone(),
            message: error.message().to_owned(),
        })?;
        if file.schema_version != MEMORY_SCHEMA_VERSION {
            return Err(MemoryError::Corrupt {
                file: shown,
                message: format!("unsupported schema_version {}", file.schema_version),
            });
        }
        Ok((path, file.entries))
    }

    fn save(&self, path: &PathBuf, entries: Vec<MemoryEntry>) -> Result<(), MemoryError> {
        let file = MemoryFile {
            schema_version: MEMORY_SCHEMA_VERSION,
            entries,
        };
        let text = toml::to_string(&file)
            .map_err(|error| MemoryError::Io(format!("cannot encode memory: {error}")))?;
        fsio::write_atomic(path, text.as_bytes())
            .map_err(|error| MemoryError::Io(format!("{}: {error}", path.display())))
    }

    fn guard(&self) -> Result<std::sync::MutexGuard<'_, ()>, MemoryError> {
        self.lock
            .lock()
            .map_err(|_| MemoryError::Io("the memory store lock was poisoned".into()))
    }
}

fn safe(id: &EntityId) -> Result<&str, MemoryError> {
    file_safe_id(id).map_err(|error| MemoryError::InvalidScope(error.to_string()))
}

impl MemoryStore for FileMemoryStore {
    fn list(&self, scope: &MemoryScope) -> Result<Vec<MemoryEntry>, MemoryError> {
        let _guard = self.guard()?;
        Ok(self.load(scope)?.1)
    }

    fn add(&self, scope: &MemoryScope, memory: NewMemory) -> Result<MemoryEntry, MemoryError> {
        let text = validate_text(&memory.text)?;
        let tags = validate_tags(&memory.tags)?;
        let _guard = self.guard()?;
        let (path, mut entries) = self.load(scope)?;
        if entries.len() >= MAX_ENTRIES_PER_SCOPE {
            return Err(MemoryError::TooMany {
                max: MAX_ENTRIES_PER_SCOPE,
            });
        }
        let now = Timestamp::now();
        let entry = MemoryEntry {
            id: EntityId::generate(MEMORY_ID_PREFIX)
                .map_err(|error| MemoryError::Io(error.to_string()))?,
            text,
            tags,
            source: memory.source,
            created_at: now.clone(),
            updated_at: now,
        };
        entries.push(entry.clone());
        self.save(&path, entries)?;
        Ok(entry)
    }

    fn edit(
        &self,
        scope: &MemoryScope,
        id: &EntityId,
        edit: MemoryEdit,
    ) -> Result<MemoryEntry, MemoryError> {
        let text = edit.text.as_deref().map(validate_text).transpose()?;
        let tags = edit.tags.as_deref().map(validate_tags).transpose()?;
        let _guard = self.guard()?;
        let (path, mut entries) = self.load(scope)?;
        let entry = entries
            .iter_mut()
            .find(|entry| &entry.id == id)
            .ok_or_else(|| MemoryError::NotFound(id.clone()))?;
        if let Some(text) = text {
            entry.text = text;
        }
        if let Some(tags) = tags {
            entry.tags = tags;
        }
        entry.source = edit.by;
        entry.updated_at = Timestamp::now();
        let updated = entry.clone();
        self.save(&path, entries)?;
        Ok(updated)
    }

    fn remove(&self, scope: &MemoryScope, id: &EntityId) -> Result<MemoryEntry, MemoryError> {
        let _guard = self.guard()?;
        let (path, mut entries) = self.load(scope)?;
        let position = entries
            .iter()
            .position(|entry| &entry.id == id)
            .ok_or_else(|| MemoryError::NotFound(id.clone()))?;
        let removed = entries.remove(position);
        self.save(&path, entries)?;
        Ok(removed)
    }

    fn clear(&self, scope: &MemoryScope) -> Result<usize, MemoryError> {
        let _guard = self.guard()?;
        let (path, entries) = self.load(scope)?;
        let count = entries.len();
        if count > 0 {
            self.save(&path, Vec::new())?;
        }
        Ok(count)
    }
}

impl std::fmt::Debug for FileMemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileMemoryStore")
            .field("dir", &self.paths.memory())
            .finish_non_exhaustive()
    }
}
