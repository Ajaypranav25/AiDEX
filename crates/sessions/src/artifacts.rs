//! Typed, content-addressed artifacts (spec §77).
//!
//! Bytes live in the session `FileStore` (`<private_sessions>/store/artifacts/<sha256>`,
//! verified on every read); SQLite records id, producer, time, kind, hash, size,
//! session, and the workspace-relative source paths. UTF-8 artifacts are redacted
//! before they are hashed and stored (`redacted` reports it); binary artifacts are
//! stored as given, so producers must never place secrets in them.

use std::{fmt, str::FromStr};

use aidex_core::{EntityId, Timestamp, WorkspacePath};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{
    SessionError, SessionStore,
    secrets::redact,
    store::rows::{self, encode, load_open_session, load_session, touch},
};

/// Largest artifact, in bytes.
pub const MAX_ARTIFACT_BYTES: usize = 256 * 1024 * 1024;
/// Most source paths recorded for one artifact.
pub const MAX_SOURCE_PATHS: usize = 1024;

/// What an artifact is. Wire form: the snake_case name, or `other:<name>`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ArtifactKind {
    Patch,
    File,
    TestReport,
    Plan,
    Benchmark,
    Screenshot,
    CommandOutput,
    ResearchNote,
    BuildLog,
    Diff,
    /// A producer-defined kind: 1-64 characters of `[a-z0-9_.-]`.
    Other(String),
}

impl ArtifactKind {
    const NAMED: [(Self, &'static str); 10] = [
        (Self::Patch, "patch"),
        (Self::File, "file"),
        (Self::TestReport, "test_report"),
        (Self::Plan, "plan"),
        (Self::Benchmark, "benchmark"),
        (Self::Screenshot, "screenshot"),
        (Self::CommandOutput, "command_output"),
        (Self::ResearchNote, "research_note"),
        (Self::BuildLog, "build_log"),
        (Self::Diff, "diff"),
    ];

    pub fn as_str(&self) -> String {
        match self {
            Self::Other(name) => format!("other:{name}"),
            known => Self::NAMED
                .iter()
                .find(|(kind, _)| kind == known)
                .map_or_else(String::new, |(_, name)| (*name).to_owned()),
        }
    }
}

impl fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

impl FromStr for ArtifactKind {
    type Err = SessionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(name) = value.strip_prefix("other:") {
            crate::model::validate_code("artifact kind", name)?;
            return Ok(Self::Other(name.to_owned()));
        }
        Self::NAMED
            .iter()
            .find(|(_, name)| *name == value)
            .map(|(kind, _)| kind.clone())
            .ok_or_else(|| {
                SessionError::InvalidArgument(format!("unknown artifact kind `{value}`"))
            })
    }
}

impl Serialize for ArtifactKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for ArtifactKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

/// Input of [`SessionStore::put_artifact`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewArtifact {
    pub kind: ArtifactKind,
    /// The agent, tool, or skill that produced it.
    pub producer: EntityId,
    pub bytes: Vec<u8>,
    pub source_paths: Vec<WorkspacePath>,
}

impl NewArtifact {
    pub fn new(kind: ArtifactKind, producer: EntityId, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            kind,
            producer,
            bytes: bytes.into(),
            source_paths: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_sources(mut self, paths: Vec<WorkspacePath>) -> Self {
        self.source_paths = paths;
        self
    }
}

/// A stored artifact's metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    /// `art_<ULID>`.
    pub id: EntityId,
    /// Lowercase hex SHA-256 of the stored bytes.
    pub sha256: String,
    pub size: u64,
    pub kind: ArtifactKind,
    pub producer: EntityId,
    pub created_at: Timestamp,
    pub session_id: EntityId,
    pub source_paths: Vec<WorkspacePath>,
    /// Whether credential redaction changed the bytes before storage.
    pub redacted: bool,
}

const ARTIFACT_COLUMNS: &str =
    "id, session_id, kind, producer, sha256, size, source_paths_json, redacted, created_at";

type ArtifactRow = (
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    bool,
    String,
);

fn artifact_row(row: &Row<'_>) -> rusqlite::Result<ArtifactRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

fn decode(
    (id, session, kind, producer, sha256, size, sources, redacted, created): ArtifactRow,
) -> Result<ArtifactRef, SessionError> {
    Ok(ArtifactRef {
        id: rows::id(id, "artifact id")?,
        sha256,
        size: rows::unsigned(size, "artifact size")?,
        kind: kind
            .parse()
            .map_err(|_| rows::corrupt("invalid artifact kind"))?,
        producer: rows::id(producer, "artifact producer")?,
        created_at: rows::time(created, "artifact time")?,
        session_id: rows::id(session, "artifact session id")?,
        source_paths: rows::json(&sources, "artifact source paths")?,
        redacted,
    })
}

pub(crate) fn load_artifact(
    connection: &Connection,
    artifact_id: &EntityId,
) -> Result<ArtifactRef, SessionError> {
    connection
        .query_row(
            &format!("SELECT {ARTIFACT_COLUMNS} FROM artifacts WHERE id = ?1"),
            [artifact_id.as_str()],
            artifact_row,
        )
        .optional()?
        .ok_or_else(|| SessionError::not_found("artifact", artifact_id))
        .and_then(decode)
}

impl SessionStore {
    /// Store an artifact for the session and return its reference.
    pub fn put_artifact(
        &self,
        id: &EntityId,
        artifact: NewArtifact,
    ) -> Result<ArtifactRef, SessionError> {
        if artifact.bytes.len() > MAX_ARTIFACT_BYTES {
            return Err(SessionError::InvalidArgument(format!(
                "artifact is {} bytes; the limit is {MAX_ARTIFACT_BYTES}",
                artifact.bytes.len()
            )));
        }
        if artifact.source_paths.len() > MAX_SOURCE_PATHS {
            return Err(SessionError::InvalidArgument(format!(
                "at most {MAX_SOURCE_PATHS} source paths may be recorded"
            )));
        }
        let (bytes, redacted) = match std::str::from_utf8(&artifact.bytes) {
            Ok(text) => match redact(text) {
                (clean, true) => (clean.into_bytes(), true),
                (_, false) => (artifact.bytes, false),
            },
            Err(_) => (artifact.bytes, false),
        };
        load_open_session(&self.lock(), id)?;
        // Content-addressed and idempotent: an orphaned blob from a failed insert is
        // harmless and reused by the next identical put.
        let sha256 = self.blobs.put_artifact(&bytes)?;
        let reference = ArtifactRef {
            id: EntityId::generate("art").map_err(|error| {
                SessionError::Unavailable(format!("cannot generate an artifact id: {error}"))
            })?,
            sha256,
            size: bytes.len() as u64,
            kind: artifact.kind,
            producer: artifact.producer,
            created_at: Timestamp::now(),
            session_id: id.clone(),
            source_paths: artifact.source_paths,
            redacted,
        };
        let connection = self.lock();
        load_open_session(&connection, id)?;
        connection.execute(
            "INSERT INTO artifacts (id, session_id, kind, producer, sha256, size, source_paths_json, redacted, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                reference.id.as_str(),
                id.as_str(),
                reference.kind.as_str(),
                reference.producer.as_str(),
                reference.sha256,
                rows::signed(reference.size),
                encode(&reference.source_paths)?,
                redacted,
                reference.created_at.as_str(),
            ],
        )?;
        touch(&connection, id, &reference.created_at)?;
        Ok(reference)
    }

    /// An artifact's metadata by id (artifact ids are unique across sessions, so a
    /// branch can read what its parent produced).
    pub fn artifact(&self, artifact_id: &EntityId) -> Result<ArtifactRef, SessionError> {
        load_artifact(&self.lock(), artifact_id)
    }

    /// An artifact's bytes, verified against the recorded SHA-256.
    pub fn read_artifact(&self, artifact_id: &EntityId) -> Result<Vec<u8>, SessionError> {
        let reference = self.artifact(artifact_id)?;
        let bytes = self.blobs.get_artifact(&reference.sha256)?;
        if bytes.len() as u64 != reference.size {
            return Err(SessionError::Corrupt(format!(
                "artifact `{artifact_id}` size does not match its record"
            )));
        }
        Ok(bytes)
    }

    /// Every artifact of the session in creation order.
    pub fn artifacts(&self, id: &EntityId) -> Result<Vec<ArtifactRef>, SessionError> {
        let connection = self.lock();
        load_session(&connection, id)?;
        let mut statement = connection.prepare(&format!(
            "SELECT {ARTIFACT_COLUMNS} FROM artifacts WHERE session_id = ?1 ORDER BY rowid"
        ))?;
        let rows = statement
            .query_map([id.as_str()], artifact_row)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(decode).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_through_their_wire_names() {
        for (kind, _) in ArtifactKind::NAMED {
            assert_eq!(kind.as_str().parse::<ArtifactKind>().unwrap(), kind);
        }
        let other = ArtifactKind::Other("trace.json".into());
        assert_eq!(other.as_str(), "other:trace.json");
        assert_eq!(
            serde_json::to_string(&other).unwrap(),
            "\"other:trace.json\""
        );
        assert!("other:Bad Name".parse::<ArtifactKind>().is_err());
        assert!("binary".parse::<ArtifactKind>().is_err());
    }
}
