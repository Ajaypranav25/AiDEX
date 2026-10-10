//! `app/current.json`: the drive's atomic pointer to the active bundle of each target.
//!
//! ```json
//! {
//!   "schema_version": 1,
//!   "targets": {
//!     "windows-x64": {
//!       "version": "0.1.0",
//!       "dir": "versions/0.1.0-windows-x64",
//!       "manifest_sha256": "<sha256 of that bundle.json>",
//!       "previous": null
//!     }
//!   }
//! }
//! ```
//!
//! One drive can carry bundles for several hosts, so entries are keyed by target.
//! Keys must look like target names but unknown ones are preserved, so an older
//! installer never drops a newer target's entry. The file is only ever replaced by
//! write-to-temporary, fsync, and rename; readers see the old or the new pointer.

use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Read},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    BundleError, DocumentError, Target,
    digest::is_sha256_hex,
    fsutil::write_atomic,
    manifest::parse_versioned,
    path::{parse_pointer_dir, validate_version},
};

/// File name of the pointer inside the drive's `app/` directory.
pub const POINTER_FILE_NAME: &str = "current.json";
/// The pointer schema this build writes and reads.
pub const POINTER_SCHEMA_VERSION: u32 = 1;
const MAX_POINTER_BYTES: u64 = 1024 * 1024;

/// One installed bundle: its version, directory, and pinned manifest digest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRef {
    pub version: String,
    /// `versions/<name>`, relative to the drive's `app/` directory.
    pub dir: String,
    /// SHA-256 of the bundle's `bundle.json`; the launcher refuses a different manifest.
    pub manifest_sha256: String,
}

impl VersionRef {
    /// The directory name under `app/versions/`.
    pub fn dir_name(&self) -> Result<&str, DocumentError> {
        parse_pointer_dir(&self.dir).map_err(DocumentError::Invalid)
    }

    fn validate(&self) -> Result<(), DocumentError> {
        validate_version(&self.version).map_err(DocumentError::Invalid)?;
        self.dir_name()?;
        if !is_sha256_hex(&self.manifest_sha256) {
            return Err(DocumentError::Invalid(format!(
                "manifest_sha256 for `{}` is not a lowercase hex sha256",
                self.dir
            )));
        }
        Ok(())
    }
}

/// The pointer entry of one target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetPointer {
    pub version: String,
    /// `versions/<name>`, relative to the drive's `app/` directory.
    pub dir: String,
    pub manifest_sha256: String,
    /// The bundle that was active before this one (rollback candidate for the updater).
    pub previous: Option<VersionRef>,
}

impl TargetPointer {
    /// The active bundle as a [`VersionRef`].
    pub fn current(&self) -> VersionRef {
        VersionRef {
            version: self.version.clone(),
            dir: self.dir.clone(),
            manifest_sha256: self.manifest_sha256.clone(),
        }
    }

    /// The directory name under `app/versions/`.
    pub fn dir_name(&self) -> Result<&str, DocumentError> {
        parse_pointer_dir(&self.dir).map_err(DocumentError::Invalid)
    }
}

/// The parsed `app/current.json`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentPointer {
    pub schema_version: u32,
    pub targets: BTreeMap<String, TargetPointer>,
}

impl Default for CurrentPointer {
    fn default() -> Self {
        Self {
            schema_version: POINTER_SCHEMA_VERSION,
            targets: BTreeMap::new(),
        }
    }
}

impl CurrentPointer {
    /// Parse and validate pointer bytes.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DocumentError> {
        let pointer: Self = parse_versioned(bytes, POINTER_SCHEMA_VERSION)?;
        pointer.validate()?;
        Ok(pointer)
    }

    /// Validate and serialize as pretty JSON with a trailing newline.
    pub fn to_json(&self) -> Result<Vec<u8>, DocumentError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| DocumentError::Malformed(error.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn validate(&self) -> Result<(), DocumentError> {
        if self.schema_version != POINTER_SCHEMA_VERSION {
            return Err(DocumentError::UnsupportedSchema {
                found: Some(u64::from(self.schema_version)),
                supported: POINTER_SCHEMA_VERSION,
            });
        }
        for (key, entry) in &self.targets {
            let well_formed = !key.is_empty()
                && key.len() <= 32
                && key.split('-').all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                });
            if !well_formed {
                return Err(DocumentError::Invalid(format!(
                    "target key `{key}` is not a target name"
                )));
            }
            entry.current().validate()?;
            if let Some(previous) = &entry.previous {
                previous.validate()?;
            }
        }
        Ok(())
    }

    /// The entry for `target`.
    pub fn get(&self, target: Target) -> Option<&TargetPointer> {
        self.targets.get(target.as_str())
    }

    /// Target keys present in the pointer.
    pub fn installed_targets(&self) -> Vec<String> {
        self.targets.keys().cloned().collect()
    }

    /// Make `next` the active bundle of `target`. The old active bundle becomes
    /// `previous` unless it is the same directory, in which case `previous` is kept.
    pub fn switch(&mut self, target: Target, next: VersionRef) {
        let previous = match self.targets.get(target.as_str()) {
            Some(old) if old.dir == next.dir => old.previous.clone(),
            Some(old) => Some(old.current()),
            None => None,
        };
        self.targets.insert(
            target.as_str().to_owned(),
            TargetPointer {
                version: next.version,
                dir: next.dir,
                manifest_sha256: next.manifest_sha256,
                previous,
            },
        );
    }

    /// Read a pointer file; `Ok(None)` when it does not exist. The file must be a
    /// regular file (not a link) of at most 1 MiB.
    pub fn load(path: &Path) -> Result<Option<Self>, BundleError> {
        let pointer_error = |error: DocumentError| BundleError::Pointer {
            path: path.to_path_buf(),
            error,
        };
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(BundleError::io(
                    format!("cannot inspect `{}`", path.display()),
                    error,
                ));
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(pointer_error(DocumentError::Malformed(
                "not a regular file".into(),
            )));
        }
        if metadata.len() > MAX_POINTER_BYTES {
            return Err(pointer_error(DocumentError::Malformed(format!(
                "larger than {MAX_POINTER_BYTES} bytes"
            ))));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .and_then(|file| file.take(MAX_POINTER_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|error| BundleError::io(format!("cannot read `{}`", path.display()), error))?;
        Self::from_json(&bytes).map(Some).map_err(pointer_error)
    }

    /// Atomically replace the pointer file at `path` (temporary file, fsync, rename).
    pub fn store(&self, path: &Path) -> Result<(), BundleError> {
        let bytes = self.to_json().map_err(|error| BundleError::Pointer {
            path: path.to_path_buf(),
            error,
        })?;
        write_atomic(path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::sha256_hex;
    use serde_json::Value;

    fn reference(version: &str) -> VersionRef {
        VersionRef {
            version: version.into(),
            dir: format!("versions/{version}-windows-x64"),
            manifest_sha256: sha256_hex(version.as_bytes()),
        }
    }

    #[test]
    fn switching_records_the_previous_bundle() {
        let mut pointer = CurrentPointer::default();
        pointer.switch(Target::WindowsX64, reference("0.1.0"));
        assert_eq!(pointer.get(Target::WindowsX64).unwrap().previous, None);
        pointer.switch(Target::WindowsX64, reference("0.2.0"));
        let entry = pointer.get(Target::WindowsX64).unwrap();
        assert_eq!(entry.version, "0.2.0");
        assert_eq!(entry.previous, Some(reference("0.1.0")));
        // Re-activating the same directory keeps the recorded rollback candidate.
        pointer.switch(Target::WindowsX64, reference("0.2.0"));
        assert_eq!(
            pointer.get(Target::WindowsX64).unwrap().previous,
            Some(reference("0.1.0"))
        );
        assert_eq!(pointer.get(Target::LinuxX64), None);
    }

    #[test]
    fn round_trips_and_preserves_unknown_targets() {
        let mut pointer = CurrentPointer::default();
        pointer.switch(Target::WindowsX64, reference("0.1.0"));
        let mut json: Value = serde_json::from_slice(&pointer.to_json().unwrap()).unwrap();
        json["targets"]["freebsd-x64"] =
            serde_json::to_value(pointer.get(Target::WindowsX64).unwrap()).unwrap();
        let parsed = CurrentPointer::from_json(json.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.installed_targets(), ["freebsd-x64", "windows-x64"]);
        assert_eq!(
            CurrentPointer::from_json(&parsed.to_json().unwrap()).unwrap(),
            parsed
        );
    }

    #[test]
    fn closed_schema_rejects_unknown_fields_and_bad_values() {
        let good = CurrentPointer::default();
        let mut pointer = good.clone();
        pointer.switch(Target::LinuxX64, reference("0.1.0"));
        let base: Value = serde_json::from_slice(&pointer.to_json().unwrap()).unwrap();

        let mut extra = base.clone();
        extra["unexpected"] = Value::Bool(true);
        assert!(matches!(
            CurrentPointer::from_json(extra.to_string().as_bytes()),
            Err(DocumentError::Malformed(_))
        ));
        let mut nested = base.clone();
        nested["targets"]["linux-x64"]["channel"] = "beta".into();
        assert!(matches!(
            CurrentPointer::from_json(nested.to_string().as_bytes()),
            Err(DocumentError::Malformed(_))
        ));
        let mut newer = base.clone();
        newer["schema_version"] = 2.into();
        assert!(matches!(
            CurrentPointer::from_json(newer.to_string().as_bytes()),
            Err(DocumentError::UnsupportedSchema { found: Some(2), .. })
        ));
        for dir in [
            "../../evil",
            "versions/../x",
            "versions/a/b",
            "/abs",
            "versions/.staging",
        ] {
            let mut escaping = base.clone();
            escaping["targets"]["linux-x64"]["dir"] = dir.into();
            assert!(
                matches!(
                    CurrentPointer::from_json(escaping.to_string().as_bytes()),
                    Err(DocumentError::Invalid(_))
                ),
                "{dir}"
            );
        }
        let mut bad_key = base.clone();
        bad_key["targets"]["Linux X64"] = base["targets"]["linux-x64"].clone();
        assert!(CurrentPointer::from_json(bad_key.to_string().as_bytes()).is_err());
        let mut bad_digest = base;
        bad_digest["targets"]["linux-x64"]["manifest_sha256"] = "abc".into();
        assert!(CurrentPointer::from_json(bad_digest.to_string().as_bytes()).is_err());
    }
}
