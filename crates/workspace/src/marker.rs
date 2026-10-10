//! AiDEX ownership markers for drive and ephemeral workspace directories.
//!
//! Every directory AiDEX creates for workspace data carries a marker naming the
//! workspace it belongs to. Deletion is refused unless the marker is present, is a
//! regular file, and names the expected workspace and kind.

use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::Path,
};

use aidex_core::{EntityId, Timestamp};
use serde::{Deserialize, Serialize};

use crate::{WorkspaceError, fsutil};

/// File name of the ownership marker inside an AiDEX-created workspace directory.
pub const MARKER_FILE: &str = "aidex-workspace.json";
const MARKER_FORMAT: &str = "aidex.workspace-owner";
const MARKER_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    format: String,
    version: u32,
    workspace_id: EntityId,
    kind: String,
    created_at: Timestamp,
}

/// Create the marker in a directory AiDEX just created. Fails if one exists.
pub(crate) fn write(dir: &Path, id: &EntityId, kind: &str) -> Result<(), WorkspaceError> {
    let marker = Marker {
        format: MARKER_FORMAT.into(),
        version: MARKER_VERSION,
        workspace_id: id.clone(),
        kind: kind.into(),
        created_at: Timestamp::now(),
    };
    let bytes = serde_json::to_vec_pretty(&marker)
        .map_err(|error| WorkspaceError::Corrupt(format!("cannot encode marker: {error}")))?;
    let path = dir.join(MARKER_FILE);
    let io = |error| WorkspaceError::io(format!("cannot write `{}`", path.display()), error);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(io)?;
    file.write_all(&bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    Ok(())
}

/// Verify that `dir` is a real directory carrying this workspace's marker.
pub(crate) fn verify(dir: &Path, id: &EntityId, kind: &str) -> Result<(), WorkspaceError> {
    let refuse = || WorkspaceError::OwnershipMarker(dir.to_path_buf());
    fsutil::require_real_dir(dir).map_err(|_| refuse())?;
    let path = dir.join(MARKER_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(refuse()),
        Err(error) => {
            return Err(WorkspaceError::io(
                format!("cannot inspect `{}`", path.display()),
                error,
            ));
        }
    };
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err(refuse());
    }
    let bytes = fs::read(&path)
        .map_err(|error| WorkspaceError::io(format!("cannot read `{}`", path.display()), error))?;
    let marker: Marker = serde_json::from_slice(&bytes).map_err(|_| refuse())?;
    if marker.format != MARKER_FORMAT
        || marker.version != MARKER_VERSION
        || marker.workspace_id != *id
        || marker.kind != kind
    {
        return Err(refuse());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_verify_only_for_the_owning_workspace_and_kind() {
        let dir = tempfile::tempdir().unwrap();
        let id = EntityId::new("wsp_a").unwrap();
        assert!(matches!(
            verify(dir.path(), &id, "drive"),
            Err(WorkspaceError::OwnershipMarker(_))
        ));
        write(dir.path(), &id, "drive").unwrap();
        verify(dir.path(), &id, "drive").unwrap();
        assert!(verify(dir.path(), &EntityId::new("wsp_b").unwrap(), "drive").is_err());
        assert!(verify(dir.path(), &id, "ephemeral").is_err());
        assert!(
            write(dir.path(), &id, "drive").is_err(),
            "markers are never replaced"
        );
        fs::write(dir.path().join(MARKER_FILE), b"{\"format\":\"other\"}").unwrap();
        assert!(verify(dir.path(), &id, "drive").is_err());
    }
}
