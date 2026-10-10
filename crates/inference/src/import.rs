//! Model pack import (spec §66): hash, identify, check compatibility, store on
//! the drive, and register — never overwriting an existing model.
//!
//! The source is copied into `models/.staging/<id>-<random>.partial` while it
//! is hashed, fsynced, re-read to verify the copy, and only then moved into a
//! freshly created `models/<id>/` (whose exclusive creation claims the id).
//! The registry entry is written last. A crash at any point leaves at most a
//! `.partial` file (removed by the next import once it is stale) or, after the
//! move, an unregistered model directory that blocks reuse of the id until it
//! is removed by hand; nothing existing is ever replaced.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use aidex_core::{CancelToken, Timestamp};
use aidex_drive::DrivePaths;
use aidex_providers::{ModelRole, ToolCallSupport};
use serde::{Deserialize, Serialize};

use crate::{
    InferenceError, LICENSE_FILE, LicenseRecord, LocalModelId, MODEL_FILE, ModelCapabilityRecord,
    ModelFormat, ModelRegistry, REGISTRY_VERSION, RegistryEntry, Result, SourceRecord,
    fsutil::{self, display},
    gguf::{self, GgufMetadata, is_known_architecture},
    memory::{clamp_u32, estimate_memory},
    verify::VerificationCache,
};

/// Staging directory under `DrivePaths::models()`.
pub const STAGING_DIR: &str = ".staging";
/// A `.partial` file untouched this long belongs to an interrupted import.
pub const STALE_PARTIAL_AGE: Duration = Duration::from_secs(10 * 60);
/// Largest licence text recorded.
const MAX_LICENSE_BYTES: u64 = 1024 * 1024;
const MAX_TEXT: usize = 256;

/// Caller-supplied facts about the model being imported.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportOptions {
    /// Registry id; derived from the file name when absent.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Licence name or SPDX id; the GGUF `general.license` is used when absent.
    #[serde(default)]
    pub license: Option<String>,
    /// A licence text file copied next to the model.
    #[serde(default)]
    pub license_file: Option<PathBuf>,
    /// Where the model came from (an `https://` URL).
    #[serde(default)]
    pub source_url: Option<String>,
    /// Upstream revision (commit, tag) of the source.
    #[serde(default)]
    pub source_revision: Option<String>,
    /// Refuse the import unless the file has this SHA-256.
    #[serde(default)]
    pub expected_sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    Inspecting,
    Copying,
    Verifying,
    Registering,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportProgress {
    pub phase: ImportPhase,
    pub done_bytes: u64,
    pub total_bytes: u64,
}

/// Import `source` with default progress and no cancellation.
pub fn import_model(
    paths: &DrivePaths,
    source: &Path,
    options: &ImportOptions,
) -> Result<RegistryEntry> {
    import_model_with(paths, source, options, &mut |_| {}, &CancelToken::new())
}

pub fn import_model_with(
    paths: &DrivePaths,
    source: &Path,
    options: &ImportOptions,
    progress: &mut dyn FnMut(&ImportProgress),
    cancel: &CancelToken,
) -> Result<RegistryEntry> {
    let source_meta = fs::metadata(source)
        .map_err(|error| InferenceError::io(format!("cannot read {}", display(source)), &error))?;
    if !source_meta.is_file() {
        return Err(InferenceError::InvalidArgument(format!(
            "{} is not a file",
            display(source)
        )));
    }
    let total = source_meta.len();
    let report = |progress: &mut dyn FnMut(&ImportProgress), phase, done| {
        progress(&ImportProgress {
            phase,
            done_bytes: done,
            total_bytes: total,
        });
    };
    report(progress, ImportPhase::Inspecting, 0);

    // 1. Identify and check compatibility before copying anything.
    let metadata = gguf::read_metadata(source)?;
    check_compatible(&metadata)?;
    let file_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| MODEL_FILE.to_owned());
    let id = match &options.id {
        Some(id) => LocalModelId::new(id.clone())?,
        None => {
            let stem = source
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            LocalModelId::derive(&stem)?
        }
    };
    let expected = options
        .expected_sha256
        .as_deref()
        .map(|sha| {
            if fsutil::is_sha256_hex(sha) {
                Ok(sha.to_ascii_lowercase())
            } else {
                Err(InferenceError::InvalidArgument(
                    "expected SHA-256 must be 64 hex digits".into(),
                ))
            }
        })
        .transpose()?;
    let source_url = options
        .source_url
        .as_deref()
        .map(validate_source_url)
        .transpose()?;
    let license_text = options
        .license_file
        .as_deref()
        .map(read_license)
        .transpose()?;
    let registry = ModelRegistry::open(paths)?;
    let model_dir = registry.model_dir(&id);
    if registry.contains(&id) {
        return Err(InferenceError::AlreadyExists {
            id: id.to_string(),
            path: registry.entry_path(&id),
        });
    }
    if fs::symlink_metadata(&model_dir).is_ok() {
        return Err(InferenceError::AlreadyExists {
            id: id.to_string(),
            path: model_dir,
        });
    }

    // 2. Copy while hashing, then verify the copy.
    let staging = paths.ensure_dir(&paths.models().join(STAGING_DIR))?;
    remove_stale_partials(&staging, STALE_PARTIAL_AGE);
    let partial = staging.join(format!("{id}-{}.partial", fsutil::random_suffix()?));
    let outcome = (|| {
        let (sha256, size) = fsutil::copy_hashing(
            source,
            &partial,
            &mut |done| report(progress, ImportPhase::Copying, done),
            cancel,
        )?;
        if let Some(expected) = &expected {
            if &sha256 != expected {
                return Err(InferenceError::HashMismatch {
                    what: format!("import source {}", display(source)),
                    expected: expected.clone(),
                    actual: sha256,
                });
            }
        }
        let (copy_sha, copy_size) = fsutil::sha256_file(
            &partial,
            &mut |done| report(progress, ImportPhase::Verifying, done),
            cancel,
        )?;
        if copy_sha != sha256 || copy_size != size {
            return Err(InferenceError::HashMismatch {
                what: format!("the copy of {} on the drive", display(source)),
                expected: sha256,
                actual: copy_sha,
            });
        }
        report(progress, ImportPhase::Registering, size);

        // 3. Claim the id by creating its directory, then move the copy in.
        fs::create_dir(&model_dir).map_err(|error| match error.kind() {
            ErrorKind::AlreadyExists => InferenceError::AlreadyExists {
                id: id.to_string(),
                path: model_dir.clone(),
            },
            _ => InferenceError::io(format!("cannot create {}", display(&model_dir)), &error),
        })?;
        let model_path = model_dir.join(MODEL_FILE);
        fsutil::rename(&partial, &model_path).map_err(|error| {
            InferenceError::io(
                format!("cannot move the model into {}", display(&model_path)),
                &error,
            )
        })?;
        fsutil::sync_dir(&model_dir);
        let license_file = match &license_text {
            Some(text) => {
                let path = model_dir.join(LICENSE_FILE);
                fsutil::write_atomic(&path, text)?;
                Some(fsutil::drive_relative(paths.root(), &path)?)
            }
            None => None,
        };
        let file = fsutil::drive_relative(paths.root(), &model_path)?;
        let entry = entry_from_metadata(
            &id,
            &metadata,
            EntryFacts {
                file: file.clone(),
                sha256: sha256.clone(),
                size_bytes: size,
                display_name: options.display_name.clone(),
                license: license_record(options.license.as_deref(), &metadata, license_file),
                source: SourceRecord {
                    url: source_url.clone(),
                    revision: options.source_revision.as_deref().map(bounded),
                    original_file_name: bounded(&file_name),
                },
            },
        );
        registry.insert_new(&entry)?;
        // The copy was just hashed in full; the first load need not rehash it.
        VerificationCache::open(paths)?.record(&file, &model_path, &sha256)?;
        Ok(entry)
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&partial);
    }
    outcome
}

/// Refuse models the pinned runtime cannot run.
pub(crate) fn check_compatible(metadata: &GgufMetadata) -> Result<()> {
    if !is_known_architecture(&metadata.architecture) {
        return Err(InferenceError::IncompatibleModel {
            reason: format!(
                "architecture `{}` is not a text-generation architecture of the pinned llama.cpp runtime",
                metadata.architecture
            ),
        });
    }
    if metadata.tensor_count == 0 {
        return Err(InferenceError::IncompatibleModel {
            reason: "the file contains no tensors".into(),
        });
    }
    Ok(())
}

fn bounded(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_TEXT)
        .collect()
}

fn validate_source_url(url: &str) -> Result<String> {
    let invalid = |reason: &str| InferenceError::InvalidArgument(format!("source URL {reason}"));
    if url.len() > 2048 || url.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err(invalid("must be at most 2048 characters without spaces"));
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| invalid("must start with https:// or http://"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err(invalid("must name a host"));
    }
    if authority.contains('@') {
        return Err(invalid("must not embed credentials"));
    }
    Ok(url.to_owned())
}

fn read_license(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)
        .map_err(|error| InferenceError::io(format!("cannot read {}", display(path)), &error))?;
    if !metadata.is_file() || metadata.len() > MAX_LICENSE_BYTES {
        return Err(InferenceError::InvalidArgument(format!(
            "licence file {} must be a file of at most 1 MiB",
            display(path)
        )));
    }
    fs::read(path)
        .map_err(|error| InferenceError::io(format!("cannot read {}", display(path)), &error))
}

fn license_record(
    stated: Option<&str>,
    metadata: &GgufMetadata,
    file: Option<String>,
) -> LicenseRecord {
    match (
        stated.map(str::trim).filter(|text| !text.is_empty()),
        &metadata.license,
    ) {
        (Some(name), _) => LicenseRecord {
            name: bounded(name),
            file,
            origin: "user".into(),
        },
        (None, Some(name)) if !name.trim().is_empty() => LicenseRecord {
            name: bounded(name.trim()),
            file,
            origin: "gguf".into(),
        },
        _ => LicenseRecord {
            name: "unknown".into(),
            file,
            origin: "unknown".into(),
        },
    }
}

/// Facts about an imported file that do not come from its metadata.
pub(crate) struct EntryFacts {
    pub file: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub display_name: Option<String>,
    pub license: LicenseRecord,
    pub source: SourceRecord,
}

/// Build a registry entry from parsed metadata.
pub(crate) fn entry_from_metadata(
    id: &LocalModelId,
    metadata: &GgufMetadata,
    facts: EntryFacts,
) -> RegistryEntry {
    let label = metadata
        .name
        .clone()
        .or_else(|| metadata.basename.clone())
        .unwrap_or_else(|| id.to_string());
    let lowered = format!(
        "{} {} {}",
        label.to_lowercase(),
        metadata.basename.clone().unwrap_or_default().to_lowercase(),
        id
    );
    let code = lowered.contains("code") || lowered.contains("coder");
    let family: String = metadata
        .architecture
        .trim_end_matches(|ch: char| ch.is_ascii_digit())
        .to_owned();
    let mut roles = Vec::new();
    if code {
        roles.push(ModelRole::Code);
    }
    roles.push(ModelRole::General);
    RegistryEntry {
        version: REGISTRY_VERSION,
        id: id.clone(),
        display_name: bounded(facts.display_name.as_deref().unwrap_or(&label)),
        family: if family.is_empty() {
            metadata.architecture.clone()
        } else {
            family
        },
        architecture: metadata.architecture.clone(),
        format: ModelFormat::Gguf,
        file: facts.file,
        sha256: facts.sha256,
        size_bytes: facts.size_bytes,
        quantization: metadata.quantization.clone(),
        gguf_version: metadata.version,
        context_length: clamp_u32(metadata.context_length.unwrap_or(4096)),
        block_count: metadata.block_count.map(clamp_u32),
        embedding_length: metadata.embedding_length.map(clamp_u32),
        vocab_size: metadata.vocab_size,
        capabilities: ModelCapabilityRecord {
            chat: metadata.has_chat_template,
            code,
            tool_calls: if metadata.has_chat_template {
                ToolCallSupport::Structured
            } else {
                ToolCallSupport::None
            },
            json_schema: metadata.has_chat_template,
            template_tools: metadata.chat_template_mentions_tools,
        },
        roles,
        memory: estimate_memory(metadata, facts.size_bytes),
        license: facts.license,
        source: facts.source,
        imported_at: Timestamp::now(),
    }
}

/// Remove `.partial` files in `staging` not modified for `max_age`; returns them.
pub fn remove_stale_partials(staging: &Path, max_age: Duration) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let Ok(read) = fs::read_dir(staging) else {
        return removed;
    };
    let now = SystemTime::now();
    for item in read.flatten() {
        let path = item.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("partial") {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= max_age);
        if metadata.is_file() && stale && fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}

/// Remove interrupted imports in the drive's staging directory (cleanup hook).
pub fn cleanup_partial_imports(paths: &DrivePaths, max_age: Duration) -> Vec<PathBuf> {
    remove_stale_partials(&paths.models().join(STAGING_DIR), max_age)
}
