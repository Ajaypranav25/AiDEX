//! The drive's local model registry (spec §10.3): one closed, versioned TOML
//! entry per model at `models/registry/<id>.toml`, describing a GGUF file that
//! AiDEX imported into `models/<id>/model.gguf`.

use std::{
    fmt, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    str::FromStr,
};

use aidex_core::{EntityId, Timestamp};
use aidex_drive::DrivePaths;
use aidex_providers::{ModelId, ModelRole, ToolCallSupport};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    InferenceError, Result,
    fsutil::{self, display},
};

/// Registry entry schema version.
pub const REGISTRY_VERSION: u32 = 1;
/// Directory (under `DrivePaths::models()`) holding registry entries.
pub const REGISTRY_DIR: &str = "registry";
/// File name of an imported model inside `models/<id>/`.
pub const MODEL_FILE: &str = "model.gguf";
/// File name of a recorded licence text inside `models/<id>/`.
pub const LICENSE_FILE: &str = "LICENSE";
/// Longest model id.
const MAX_ID_LEN: usize = 64;
/// Largest registry entry read.
const MAX_ENTRY_BYTES: u64 = 64 * 1024;
/// Entries listed at most.
const MAX_ENTRIES: usize = 4096;

/// A local model id: 1-64 of `[a-z0-9._-]`, starting with a letter or digit.
/// It names the registry entry, the model directory, the provider model id
/// (`local/<id>`), and event identifiers, so it is restricted to characters
/// that are safe in all four.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalModelId {
    text: String,
    model: ModelId,
    entity: EntityId,
}

const RESERVED_WINDOWS_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

impl LocalModelId {
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        let invalid = |reason| InferenceError::InvalidModelId {
            id: id.chars().take(80).collect(),
            reason,
        };
        if id.is_empty() || id.len() > MAX_ID_LEN {
            return Err(invalid("must be 1-64 characters"));
        }
        if !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        }) {
            return Err(invalid("use lowercase letters, digits, `.`, `_`, and `-`"));
        }
        if !id.as_bytes()[0].is_ascii_alphanumeric() {
            return Err(invalid("must start with a letter or digit"));
        }
        if id.ends_with('.') || id.contains("..") {
            return Err(invalid("must not end with `.` or contain `..`"));
        }
        let stem = id.split('.').next().unwrap_or_default();
        if RESERVED_WINDOWS_NAMES.contains(&stem) {
            return Err(invalid("is a reserved Windows device name"));
        }
        let model = ModelId::new(id.clone()).map_err(|_| invalid("is not a valid model id"))?;
        let entity = EntityId::new(id.clone()).map_err(|_| invalid("is not a valid identifier"))?;
        Ok(Self {
            text: id,
            model,
            entity,
        })
    }

    /// An id derived from a file name: lowercased, other characters mapped to
    /// `-`, trimmed to 64 characters; `model` when nothing usable remains.
    pub fn derive(file_stem: &str) -> Result<Self> {
        let mut text = String::new();
        for ch in file_stem.chars().flat_map(char::to_lowercase) {
            let mapped =
                if ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_') {
                    ch
                } else {
                    '-'
                };
            if (mapped == '-' || mapped == '.') && text.ends_with(mapped) {
                continue;
            }
            text.push(mapped);
        }
        let mut text: String = text
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
            .chars()
            .take(MAX_ID_LEN)
            .collect();
        while text.ends_with(|ch: char| !ch.is_ascii_alphanumeric()) {
            text.pop();
        }
        if text.is_empty() {
            return Self::new("model");
        }
        Self::new(text.clone())
            .or_else(|_| Self::new(format!("model-{text}")))
            .or_else(|_| Self::new("model"))
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The provider-facing model id (`local/<id>`).
    pub fn model_id(&self) -> &ModelId {
        &self.model
    }

    /// The event identifier for this model.
    pub fn entity_id(&self) -> &EntityId {
        &self.entity
    }
}

impl fmt::Display for LocalModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl FromStr for LocalModelId {
    type Err = InferenceError;
    fn from_str(text: &str) -> Result<Self> {
        Self::new(text)
    }
}

impl Serialize for LocalModelId {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for LocalModelId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(text).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    Gguf,
}

/// What the model can do, as recorded at import.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCapabilityRecord {
    /// A chat template is present.
    pub chat: bool,
    /// The model is a coding model (name heuristic).
    pub code: bool,
    /// `structured`: JSON-schema constrained output through llama-server
    /// grammars. `native` only when set deliberately after validation.
    pub tool_calls: ToolCallSupport,
    pub json_schema: bool,
    /// The chat template references tools (native tool calls may work with `--jinja`).
    pub template_tools: bool,
}

/// RAM estimates (spec §10.3 `min_ram_gb`/`recommended_ram_gb`), in bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryEstimate {
    /// Model + KV cache at `min_context` + runtime overhead.
    pub min_ram_bytes: u64,
    /// Model + KV cache at `recommended_context` + runtime overhead.
    pub recommended_ram_bytes: u64,
    pub min_context: u32,
    pub recommended_context: u32,
    /// f16 K+V cache bytes per context token.
    pub kv_bytes_per_token: u64,
    /// Runtime compute buffers and overhead assumed by the estimates.
    pub overhead_bytes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseRecord {
    /// SPDX id or licence name (`unknown` when nobody stated one).
    pub name: String,
    /// Drive-relative path of the recorded licence text, when one was imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Where the licence name came from: `user`, `gguf`, or `unknown`.
    pub origin: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// The imported file's original name (not its host path).
    pub original_file_name: String,
}

/// One registered local model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryEntry {
    pub version: u32,
    pub id: LocalModelId,
    pub display_name: String,
    pub family: String,
    pub architecture: String,
    pub format: ModelFormat,
    /// Drive-relative path with `/` separators (`models/<id>/model.gguf`).
    pub file: String,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    pub gguf_version: u32,
    /// Training context length (`<arch>.context_length`).
    pub context_length: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vocab_size: Option<u64>,
    pub capabilities: ModelCapabilityRecord,
    pub roles: Vec<ModelRole>,
    pub memory: MemoryEstimate,
    pub license: LicenseRecord,
    pub source: SourceRecord,
    pub imported_at: Timestamp,
}

impl RegistryEntry {
    /// Absolute path of the model file, refusing anything outside `models/`.
    pub fn model_path(&self, paths: &DrivePaths) -> Result<PathBuf> {
        resolve_drive_file(paths, &self.file)
    }

    /// Absolute path of the recorded licence text, if any.
    pub fn license_path(&self, paths: &DrivePaths) -> Result<Option<PathBuf>> {
        self.license
            .file
            .as_deref()
            .map(|file| resolve_drive_file(paths, file))
            .transpose()
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.version != REGISTRY_VERSION {
            return Err(InferenceError::corrupt(
                path,
                format!("registry entry version {} is not supported", self.version),
            ));
        }
        if !fsutil::is_sha256_hex(&self.sha256) {
            return Err(InferenceError::corrupt(
                path,
                "sha256 is not a SHA-256 hex digest",
            ));
        }
        let expected = format!("{}.toml", self.id);
        if path.file_name().and_then(|name| name.to_str()) != Some(expected.as_str()) {
            return Err(InferenceError::corrupt(
                path,
                format!("entry id `{}` does not match its file name", self.id),
            ));
        }
        Ok(())
    }
}

/// Resolve a drive-relative `models/...` path written with `/`.
fn resolve_drive_file(paths: &DrivePaths, relative: &str) -> Result<PathBuf> {
    let models = paths.layout().public_models.as_str().to_owned();
    let mut segments = relative.split('/');
    let prefix: Vec<&str> = models.split('/').collect();
    for expected in &prefix {
        if segments.next() != Some(expected) {
            return Err(InferenceError::corrupt(
                relative,
                format!("model files must live under `{models}/`"),
            ));
        }
    }
    let mut path = paths.models();
    let mut count = 0;
    for segment in segments {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains(['\\', ':'])
        {
            return Err(InferenceError::corrupt(
                relative,
                "path contains an unsafe segment",
            ));
        }
        path.push(segment);
        count += 1;
    }
    if count == 0 {
        return Err(InferenceError::corrupt(relative, "path names no file"));
    }
    Ok(path)
}

/// A registry file that could not be used; reported next to the good entries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryProblem {
    pub file: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryListing {
    /// Entries ordered by id.
    pub entries: Vec<RegistryEntry>,
    pub problems: Vec<RegistryProblem>,
}

/// What `remove` deleted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemovedModel {
    pub id: LocalModelId,
    /// Drive-relative paths deleted.
    pub deleted: Vec<String>,
    /// Files AiDEX left in place because it did not create them.
    pub kept: Vec<String>,
}

/// The registry directory of one drive.
#[derive(Clone, Debug)]
pub struct ModelRegistry {
    paths: DrivePaths,
    dir: PathBuf,
}

impl ModelRegistry {
    /// Open (creating `models/registry/` if needed).
    pub fn open(paths: &DrivePaths) -> Result<Self> {
        let dir = paths.ensure_dir(&paths.models().join(REGISTRY_DIR))?;
        Ok(Self {
            paths: paths.clone(),
            dir,
        })
    }

    pub fn paths(&self) -> &DrivePaths {
        &self.paths
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entry_path(&self, id: &LocalModelId) -> PathBuf {
        self.dir.join(format!("{id}.toml"))
    }

    /// `models/<id>/`.
    pub fn model_dir(&self, id: &LocalModelId) -> PathBuf {
        self.paths.models().join(id.as_str())
    }

    pub fn contains(&self, id: &LocalModelId) -> bool {
        fs::symlink_metadata(self.entry_path(id)).is_ok()
    }

    pub fn get(&self, id: &LocalModelId) -> Result<RegistryEntry> {
        let path = self.entry_path(id);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(InferenceError::ModelNotFound { id: id.to_string() });
            }
            _ => {}
        }
        read_entry(&path)
    }

    /// Every entry; unreadable files are reported as problems, never hidden.
    pub fn list(&self) -> Result<RegistryListing> {
        let mut listing = RegistryListing::default();
        let read = fs::read_dir(&self.dir).map_err(|error| {
            InferenceError::io(format!("cannot list {}", display(&self.dir)), &error)
        })?;
        for item in read.take(MAX_ENTRIES) {
            let item = item.map_err(|error| {
                InferenceError::io(format!("cannot list {}", display(&self.dir)), &error)
            })?;
            let path = item.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with('.') || !name.ends_with(".toml") {
                continue;
            }
            match read_entry(&path) {
                Ok(entry) => listing.entries.push(entry),
                Err(error) => listing.problems.push(RegistryProblem {
                    file: name.to_owned(),
                    message: error.to_string(),
                }),
            }
        }
        listing.entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(listing)
    }

    /// Write a new entry; refuses to replace one.
    pub(crate) fn insert_new(&self, entry: &RegistryEntry) -> Result<()> {
        let path = self.entry_path(&entry.id);
        if fs::symlink_metadata(&path).is_ok() {
            return Err(InferenceError::AlreadyExists {
                id: entry.id.to_string(),
                path,
            });
        }
        let text = toml::to_string(entry).map_err(|error| InferenceError::Io {
            context: "cannot encode registry entry".into(),
            message: error.to_string(),
        })?;
        fsutil::write_atomic(&path, text.as_bytes())
    }

    /// Delete the entry and the files AiDEX imported for it (the model file and a
    /// recorded licence inside `models/<id>/`), then the directory if empty.
    pub(crate) fn remove(&self, id: &LocalModelId) -> Result<RemovedModel> {
        let entry = self.get(id)?;
        let root = self.paths.root().to_path_buf();
        let model_dir = self.model_dir(id);
        let mut removed = RemovedModel {
            id: id.clone(),
            deleted: Vec::new(),
            kept: Vec::new(),
        };
        let owned = |path: &Path| path.parent() == Some(model_dir.as_path());
        let mut files = vec![entry.model_path(&self.paths)?];
        if let Some(license) = entry.license_path(&self.paths)? {
            files.push(license);
        }
        for file in files {
            let relative = fsutil::drive_relative(&root, &file).unwrap_or_else(|_| display(&file));
            if !owned(&file) {
                removed.kept.push(relative);
                continue;
            }
            fsutil::refuse_symlink(&file)?;
            match fs::remove_file(&file) {
                Ok(()) => removed.deleted.push(relative),
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(InferenceError::io(
                        format!("cannot delete {}", display(&file)),
                        &error,
                    ));
                }
            }
        }
        let entry_path = self.entry_path(id);
        fs::remove_file(&entry_path).map_err(|error| {
            InferenceError::io(format!("cannot delete {}", display(&entry_path)), &error)
        })?;
        removed.deleted.push(
            fsutil::drive_relative(&root, &entry_path).unwrap_or_else(|_| display(&entry_path)),
        );
        // Only an empty directory is removed; anything the user put there stays.
        if fs::remove_dir(&model_dir).is_err() && model_dir.exists() {
            if let Ok(rest) = fs::read_dir(&model_dir) {
                for item in rest.flatten().take(64) {
                    removed.kept.push(
                        fsutil::drive_relative(&root, &item.path())
                            .unwrap_or_else(|_| display(&item.path())),
                    );
                }
            }
        }
        Ok(removed)
    }
}

fn read_entry(path: &Path) -> Result<RegistryEntry> {
    fsutil::regular_file(path, "registry entry")?;
    let metadata = fs::metadata(path)
        .map_err(|error| InferenceError::io(format!("cannot read {}", display(path)), &error))?;
    if metadata.len() > MAX_ENTRY_BYTES {
        return Err(InferenceError::corrupt(path, "registry entry is too large"));
    }
    let text = fs::read_to_string(path)
        .map_err(|error| InferenceError::io(format!("cannot read {}", display(path)), &error))?;
    let entry: RegistryEntry = toml::from_str(&text).map_err(|error| {
        let line = error
            .span()
            .map(|span| text[..span.start.min(text.len())].lines().count().max(1));
        InferenceError::corrupt(
            path,
            match line {
                Some(line) => format!("invalid registry entry near line {line}"),
                None => "invalid registry entry".into(),
            },
        )
    })?;
    entry.validate(path)?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_restricted_to_portable_names() {
        for good in ["qwen2.5-coder-0.5b", "a", "m_1", "x9"] {
            assert!(LocalModelId::new(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "Qwen",
            "-x",
            ".x",
            "x.",
            "a..b",
            "a/b",
            "a\\b",
            "con",
            "nul.gguf",
            "a b",
            &"x".repeat(65),
        ] {
            assert!(LocalModelId::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn ids_derive_from_file_names() {
        let derive = |stem: &str| LocalModelId::derive(stem).unwrap().as_str().to_owned();
        assert_eq!(
            derive("Qwen2.5-Coder-0.5B-Instruct-Q4_K_M"),
            "qwen2.5-coder-0.5b-instruct-q4_k_m"
        );
        assert_eq!(derive("  My Model!! "), "my-model");
        assert_eq!(derive("CON"), "model-con");
        assert_eq!(derive("***"), "model");
        assert_eq!(derive(&"Z".repeat(100)).len(), 64);
    }
}
