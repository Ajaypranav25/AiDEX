//! The baseline manifest recorded when an ephemeral copy is made:
//! workspace-relative path → size and SHA-256 of the copied bytes.

use std::{collections::BTreeMap, fs, io::ErrorKind, path::Path};

use aidex_core::{EntityId, Timestamp, WorkspacePath};
use serde::{Deserialize, Serialize};

use crate::{WorkspaceError, fsutil};

/// File name of the baseline manifest inside `workspaces/ephemeral/<id>/`.
pub const BASELINE_FILE: &str = "baseline.json";
const FORMAT: &str = "aidex.ephemeral-baseline";
const VERSION: u32 = 1;

/// Size and content hash of one file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileState {
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Baseline {
    format: String,
    version: u32,
    pub workspace_id: EntityId,
    pub source_id: EntityId,
    pub created_at: Timestamp,
    /// Keys are validated workspace paths; ordered for deterministic output.
    pub files: BTreeMap<String, FileState>,
}

impl Baseline {
    pub fn new(
        workspace_id: EntityId,
        source_id: EntityId,
        files: BTreeMap<String, FileState>,
    ) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            workspace_id,
            source_id,
            created_at: Timestamp::now(),
            files,
        }
    }

    /// Read and validate the baseline of `workspace_id` from `dir`.
    pub fn load(dir: &Path, workspace_id: &EntityId) -> Result<Self, WorkspaceError> {
        let path = dir.join(BASELINE_FILE);
        if fsutil::is_link(&path) {
            return Err(WorkspaceError::Corrupt(format!(
                "`{}` is a link",
                path.display()
            )));
        }
        let bytes = fs::read(&path).map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                WorkspaceError::Corrupt(format!("baseline `{}` is missing", path.display()))
            } else {
                WorkspaceError::io(format!("cannot read `{}`", path.display()), error)
            }
        })?;
        let baseline: Self = serde_json::from_slice(&bytes).map_err(|error| {
            WorkspaceError::Corrupt(format!("baseline `{}`: {error}", path.display()))
        })?;
        if baseline.format != FORMAT
            || baseline.version != VERSION
            || baseline.workspace_id != *workspace_id
        {
            return Err(WorkspaceError::Corrupt(format!(
                "baseline `{}` does not belong to workspace {workspace_id}",
                path.display()
            )));
        }
        if let Some(bad) = baseline
            .files
            .keys()
            .find(|key| WorkspacePath::new(key.as_str()).is_err())
        {
            return Err(WorkspaceError::Corrupt(format!(
                "baseline `{}` lists an invalid path `{bad}`",
                path.display()
            )));
        }
        Ok(baseline)
    }

    /// Persist atomically (temp file, fsync, rename).
    pub fn save(&self, dir: &Path) -> Result<(), WorkspaceError> {
        let path = dir.join(BASELINE_FILE);
        let bytes = serde_json::to_vec(self)
            .map_err(|error| WorkspaceError::Corrupt(format!("cannot encode baseline: {error}")))?;
        fsutil::write_atomic(&path, &bytes).map_err(|error| {
            WorkspaceError::io(format!("cannot write `{}`", path.display()), error)
        })
    }
}
