//! [`Updater`]: the drive's update service, and what every operation shares.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, TryLockError},
};

use aidex_bundle::{
    BundleError, BundleManifest, CurrentPointer, SignaturePolicy, SignatureState, Target,
    TrustStore, UpdateJournal, VerifiedBundle, VerifyScope, archive::ExtractLimits, journal_path,
    layout, read_manifest_bytes, sha256_hex, verify_bundle, verify_signature,
};
use aidex_core::{EntityId, Timestamp};
use aidex_drive::{CURRENT_SCHEMA_VERSION, DrivePaths};
use aidex_events::{EventKind, EventScope, EventSink, NullEventSink};
use serde::{Deserialize, Serialize};

use crate::{ProcessSelfTest, SelfTest, UpdateError};

/// Size limits for update packages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UpdateLimits {
    /// Largest archive accepted, checked before anything is extracted.
    pub max_archive_bytes: u64,
    /// Limits applied while extracting (entries, per-file and total bytes).
    pub extract: ExtractLimits,
}

impl Default for UpdateLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 2 * 1024 * 1024 * 1024,
            extract: ExtractLimits::default(),
        }
    }
}

/// How an [`Updater`] verifies and tests packages.
#[derive(Clone)]
pub struct UpdaterConfig {
    /// Trusted signing keys; the drive's `manifests/revoked-keys.json` is added at
    /// every operation.
    pub trust: TrustStore,
    pub self_test: Arc<dyn SelfTest>,
    pub limits: UpdateLimits,
}

impl UpdaterConfig {
    /// The binary's built-in trust store and the real process self-test.
    pub fn builtin() -> Result<Self, UpdateError> {
        Ok(Self::new(TrustStore::builtin()?))
    }

    /// `trust` with the process self-test and default limits.
    pub fn new(trust: TrustStore) -> Self {
        Self {
            trust,
            self_test: Arc::new(ProcessSelfTest::default()),
            limits: UpdateLimits::default(),
        }
    }

    pub fn with_self_test(mut self, self_test: Arc<dyn SelfTest>) -> Self {
        self.self_test = self_test;
        self
    }

    pub fn with_limits(mut self, limits: UpdateLimits) -> Self {
        self.limits = limits;
        self
    }
}

/// Facts about a verified package or installed bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CandidateInfo {
    pub version: String,
    pub target: Target,
    pub manifest_sha256: String,
    pub created_at: Timestamp,
    pub git_commit: Option<String>,
    pub signature: SignatureState,
    /// The bundle lists `docs/sbom.cdx.json`.
    pub sbom: bool,
}

/// A bundle directory that passed every check.
pub(crate) struct Candidate {
    pub bundle: VerifiedBundle,
    pub signature: SignatureState,
}

impl Candidate {
    pub fn manifest(&self) -> &BundleManifest {
        &self.bundle.manifest
    }

    pub fn info(&self) -> CandidateInfo {
        let manifest = self.manifest();
        CandidateInfo {
            version: manifest.version.clone(),
            target: manifest.target,
            manifest_sha256: self.bundle.manifest_sha256.clone(),
            created_at: manifest.created_at.clone(),
            git_commit: manifest.git_commit.clone(),
            signature: self.signature.clone(),
            sbom: manifest.find(crate::sbom::SBOM_BUNDLE_PATH).is_some(),
        }
    }
}

/// The update service for one target of one drive.
///
/// Every operation runs under an in-process gate and the drive's `app/update.lock`,
/// so at most one runs at a time. Only the daemon should own an `Updater` for a
/// drive: [`recover`](Self::recover) assumes no other process is updating it.
pub struct Updater {
    pub(crate) paths: DrivePaths,
    pub(crate) target: Target,
    pub(crate) config: UpdaterConfig,
    pub(crate) events: Arc<dyn EventSink>,
    gate: Mutex<()>,
}

impl Updater {
    pub fn new(paths: DrivePaths, target: Target, config: UpdaterConfig) -> Self {
        Self {
            paths,
            target,
            config,
            events: Arc::new(NullEventSink),
            gate: Mutex::new(()),
        }
    }

    /// An updater for this host's target.
    pub fn for_host(paths: DrivePaths, config: UpdaterConfig) -> Result<Self, UpdateError> {
        let target = Target::host().ok_or(UpdateError::UnsupportedHost)?;
        Ok(Self::new(paths, target, config))
    }

    /// Emit `update.*` events to `events` (best effort: a failed emit never fails an
    /// update).
    pub fn with_events(mut self, events: Arc<dyn EventSink>) -> Self {
        self.events = events;
        self
    }

    pub fn paths(&self) -> &DrivePaths {
        &self.paths
    }

    pub fn target(&self) -> Target {
        self.target
    }

    pub(crate) fn gate(&self) -> Result<MutexGuard<'_, ()>, UpdateError> {
        match self.gate.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => Err(UpdateError::Busy {
                holder: "another operation of this process".into(),
            }),
        }
    }

    pub(crate) fn emit(&self, event: EventKind) {
        let _ = self.events.emit(EventScope::global(), event);
    }

    pub(crate) fn emit_failed(&self, id: Option<&EntityId>, stage: &str, error: &UpdateError) {
        self.emit(EventKind::UpdateFailed {
            update_id: id.cloned(),
            stage: stage.to_owned(),
            code: error.reason().to_owned(),
            message: error.to_string(),
        });
    }

    /// The trust store plus the drive's revocations, and the unsigned opt-in.
    pub(crate) fn policy(&self, allow_unsigned: bool) -> Result<SignaturePolicy, UpdateError> {
        let trust = self
            .config
            .trust
            .clone()
            .with_drive_revocations(&self.paths)?;
        Ok(SignaturePolicy {
            trust,
            allow_unsigned,
        })
    }

    pub(crate) fn app(&self) -> Result<PathBuf, UpdateError> {
        Ok(self.paths.ensure_dir(&self.paths.app())?)
    }

    pub(crate) fn pointer_file(&self) -> PathBuf {
        layout::pointer_path(&self.paths)
    }

    pub(crate) fn journal_file(&self) -> PathBuf {
        journal_path(&self.paths)
    }

    pub(crate) fn load_pointer(&self) -> Result<Option<CurrentPointer>, UpdateError> {
        Ok(CurrentPointer::load(&self.pointer_file())?)
    }

    /// Refuse to start while an earlier operation is unfinished.
    pub(crate) fn ensure_no_pending(&self) -> Result<(), UpdateError> {
        if let Some(journal) = UpdateJournal::load(&self.journal_file())?
            && !journal.state.is_terminal()
        {
            return Err(UpdateError::RecoveryRequired {
                state: journal.state,
            });
        }
        Ok(())
    }

    /// Check the bundle in `dir`: manifest, target, drive schema, host extraction,
    /// signature (before hashing anything), then every file. `pinned` is the expected
    /// `bundle.json` digest for installed bundles.
    pub(crate) fn verify_dir(
        &self,
        dir: &Path,
        policy: &SignaturePolicy,
        pinned: Option<&str>,
        apply_modes: bool,
    ) -> Result<Candidate, UpdateError> {
        let bytes = read_manifest_bytes(dir)?;
        let manifest_sha256 = sha256_hex(&bytes);
        if let Some(pinned) = pinned
            && pinned != manifest_sha256
        {
            return Err(UpdateError::Tampered {
                path: aidex_bundle::MANIFEST_FILE_NAME.into(),
                problem: aidex_bundle::IntegrityProblem::ManifestPin {
                    expected: pinned.to_owned(),
                    actual: manifest_sha256,
                },
            });
        }
        let manifest =
            BundleManifest::from_json(&bytes).map_err(|error| BundleError::Manifest {
                path: dir.join(aidex_bundle::MANIFEST_FILE_NAME),
                error,
            })?;
        if manifest.target != self.target {
            return Err(UpdateError::WrongTarget {
                expected: self.target,
                found: manifest.target,
            });
        }
        if manifest.min_drive_schema > CURRENT_SCHEMA_VERSION {
            return Err(UpdateError::IncompatibleDriveSchema {
                required: manifest.min_drive_schema,
                found: CURRENT_SCHEMA_VERSION,
            });
        }
        if manifest.state.host_extraction {
            return Err(UpdateError::HostExtraction);
        }
        let critical: Vec<String> = manifest
            .critical_files()
            .map(|file| file.path.clone())
            .collect();
        let classify = |error| UpdateError::from_package(error, &critical);
        let signature =
            verify_signature(dir, &bytes, &manifest.created_at, policy).map_err(classify)?;
        if apply_modes {
            aidex_bundle::steps::apply_modes(dir, &manifest).map_err(classify)?;
        }
        let bundle = verify_bundle(dir, VerifyScope::All).map_err(classify)?;
        if bundle.manifest_sha256 != manifest_sha256 {
            return Err(UpdateError::Tampered {
                path: aidex_bundle::MANIFEST_FILE_NAME.into(),
                problem: aidex_bundle::IntegrityProblem::ManifestPin {
                    expected: manifest_sha256,
                    actual: bundle.manifest_sha256,
                },
            });
        }
        Ok(Candidate { bundle, signature })
    }

    /// `app/versions/<dir name>` of a pointer `dir` value.
    pub(crate) fn version_path(&self, dir: &str) -> Result<PathBuf, UpdateError> {
        let name = aidex_bundle::parse_pointer_dir(dir).map_err(|reason| BundleError::Pointer {
            path: self.pointer_file(),
            error: aidex_bundle::DocumentError::Invalid(reason),
        })?;
        Ok(layout::versions_dir(&self.paths).join(name))
    }
}

/// Where the running executable's bundle lives, as a pointer `dir` value, if it runs
/// from `app/versions/<name>/bin/` of this drive.
pub(crate) fn running_bundle_dir(paths: &DrivePaths) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(exe).ok()?;
    let versions = std::fs::canonicalize(layout::versions_dir(paths)).ok()?;
    let relative = exe.strip_prefix(&versions).ok()?;
    let name = relative.components().next()?.as_os_str().to_str()?;
    Some(aidex_bundle::pointer_dir(name))
}
