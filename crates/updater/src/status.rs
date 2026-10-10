//! Installed-bundle integrity (`aidex doctor`, `aidex update verify`) and update status.

use std::fs;

use aidex_bundle::{
    BundleError, IntegrityProblem, JournalOperation, JournalState, SignaturePolicy, SignatureState,
    Target, TrustStore, UpdateJournal, VerifyScope, VersionRef, journal_path, layout,
    resolve_active, sha256_hex,
};
use aidex_core::{EntityId, Timestamp};
use aidex_drive::DrivePaths;
use serde::{Deserialize, Serialize};

use crate::{
    UpdateError, Updater,
    sbom::{SBOM_BUNDLE_PATH, Sbom, SbomSummary},
    service::running_bundle_dir,
};

/// Whether the active bundle carries a valid SBOM.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SbomStatus {
    /// The manifest lists `docs/sbom.cdx.json`.
    pub present: bool,
    /// The file matches its manifest digest and is a valid CycloneDX 1.5 document.
    pub valid: bool,
    pub summary: Option<SbomSummary>,
    pub error: Option<String>,
}

/// The integrity of the active bundle on a drive.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub target: Option<Target>,
    /// `app/current.json` has an entry for the target.
    pub installed: bool,
    /// The bundle, its signature, and every file in `scope` verified.
    pub verified: bool,
    pub scope: VerifyScope,
    pub version: Option<String>,
    pub dir: Option<String>,
    pub manifest_sha256: Option<String>,
    pub verified_files: Option<usize>,
    pub signature: Option<SignatureState>,
    pub signer_key_id: Option<String>,
    pub sbom: SbomStatus,
    pub previous: Option<VersionRef>,
    /// The state of an unfinished update journal, if any.
    pub pending_update: Option<JournalState>,
    /// Stable reason when not verified (`not_installed`, `unsigned`, `tampered`, ...).
    pub reason: Option<String>,
    pub error: Option<String>,
}

/// Verify the active bundle of `target` on `paths` with `trust` (plus the drive's
/// revocations), exactly as the launcher does, and inspect its SBOM. Never fails:
/// problems are reported in the result.
pub fn integrity_report(
    paths: &DrivePaths,
    target: Target,
    trust: &TrustStore,
    scope: VerifyScope,
    allow_unsigned: bool,
) -> IntegrityReport {
    let mut report = IntegrityReport {
        target: Some(target),
        installed: false,
        verified: false,
        scope,
        version: None,
        dir: None,
        manifest_sha256: None,
        verified_files: None,
        signature: None,
        signer_key_id: None,
        sbom: SbomStatus::default(),
        previous: None,
        pending_update: match UpdateJournal::load(&journal_path(paths)) {
            Ok(Some(journal)) if !journal.state.is_terminal() => Some(journal.state),
            _ => None,
        },
        reason: None,
        error: None,
    };
    if let Ok(Some(pointer)) = aidex_bundle::CurrentPointer::load(&layout::pointer_path(paths)) {
        if let Some(entry) = pointer.get(target) {
            report.installed = true;
            report.version = Some(entry.version.clone());
            report.dir = Some(entry.dir.clone());
            report.manifest_sha256 = Some(entry.manifest_sha256.clone());
            report.previous = entry.previous.clone();
        }
    }
    let policy = match trust.clone().with_drive_revocations(paths) {
        Ok(trust) => SignaturePolicy {
            trust,
            allow_unsigned,
        },
        Err(error) => {
            report.reason = Some("revocations".into());
            report.error = Some(error.to_string());
            return report;
        }
    };
    match resolve_active(paths, target, scope, &policy) {
        Ok(active) => {
            report.verified = true;
            report.verified_files = Some(active.bundle.verified_files);
            report.signer_key_id = active.signature.key_id().map(str::to_owned);
            report.signature = Some(active.signature.clone());
            report.sbom = sbom_status(&active.bundle.dir, &active.bundle.manifest);
        }
        Err(error) => {
            report.reason = Some(bundle_reason(&error).to_owned());
            report.error = Some(error.to_string());
        }
    }
    report
}

fn bundle_reason(error: &BundleError) -> &'static str {
    match error {
        BundleError::NotInstalled { .. } | BundleError::TargetNotInstalled { .. } => {
            "not_installed"
        }
        BundleError::Signature { error, .. } => error.reason(),
        BundleError::Integrity {
            problem: IntegrityProblem::ManifestPin { .. },
            ..
        } => "manifest_pin",
        BundleError::Integrity { .. } => "tampered",
        BundleError::WrongTarget { .. } => "wrong_target",
        BundleError::DriveSchemaTooOld { .. } => "incompatible_drive_schema",
        BundleError::Trust(_) => "revocations",
        other => match other.kind() {
            aidex_bundle::BundleErrorKind::Incompatible => "incompatible",
            aidex_bundle::BundleErrorKind::Integrity => "integrity",
            _ => "error",
        },
    }
}

/// Check the SBOM a bundle lists against its digest and the CycloneDX schema rules.
pub fn sbom_status(
    bundle_dir: &std::path::Path,
    manifest: &aidex_bundle::BundleManifest,
) -> SbomStatus {
    let Some(entry) = manifest.find(SBOM_BUNDLE_PATH) else {
        return SbomStatus::default();
    };
    let path = SBOM_BUNDLE_PATH
        .split('/')
        .fold(bundle_dir.to_path_buf(), |path, part| path.join(part));
    let checked = fs::read(&path)
        .map_err(|error| format!("cannot read {SBOM_BUNDLE_PATH}: {error}"))
        .and_then(|bytes| {
            if sha256_hex(&bytes) != entry.sha256 {
                return Err(format!(
                    "{SBOM_BUNDLE_PATH} does not match its manifest digest"
                ));
            }
            Sbom::from_json(&bytes)?.validate()
        });
    match checked {
        Ok(summary) => SbomStatus {
            present: true,
            valid: true,
            summary: Some(summary),
            error: None,
        },
        Err(error) => SbomStatus {
            present: true,
            valid: false,
            summary: None,
            error: Some(error),
        },
    }
}

/// The last or current operation recorded in the journal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JournalSummary {
    pub id: EntityId,
    pub operation: JournalOperation,
    pub state: JournalState,
    pub started_at: Timestamp,
    pub updated_at: Timestamp,
    pub candidate_version: Option<String>,
    pub to: Option<VersionRef>,
    pub signer_key_id: Option<String>,
    pub outcome: Option<String>,
}

impl From<UpdateJournal> for JournalSummary {
    fn from(journal: UpdateJournal) -> Self {
        Self {
            id: journal.id,
            operation: journal.operation,
            state: journal.state,
            started_at: journal.started_at,
            updated_at: journal.updated_at,
            candidate_version: journal.candidate_version,
            to: journal.to,
            signer_key_id: journal.signer_key_id,
            outcome: journal.outcome,
        }
    }
}

/// [`Updater::status`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub target: Target,
    pub current: Option<VersionRef>,
    pub previous: Option<VersionRef>,
    /// Integrity of the active bundle (critical files, signature, SBOM).
    pub integrity: IntegrityReport,
    /// The last (or unfinished) operation.
    pub journal: Option<JournalSummary>,
    /// An operation is unfinished and needs recovery.
    pub pending: bool,
    /// Directories under `app/versions/`.
    pub versions: Vec<String>,
    /// The bundle directory the calling process runs from, when it runs from this
    /// drive's `app/versions/`.
    pub running_dir: Option<String>,
    /// The running process is not the version `app/current.json` selects: restart
    /// AiDEX to use it.
    pub restart_required: bool,
}

impl Updater {
    /// Current, previous, integrity, last operation, and installed versions.
    pub fn status(&self) -> Result<UpdateStatus, UpdateError> {
        let pointer = self.load_pointer()?;
        let entry = pointer
            .as_ref()
            .and_then(|pointer| pointer.get(self.target));
        let journal = match UpdateJournal::load(&self.journal_file()) {
            Ok(journal) => journal,
            Err(BundleError::Journal { .. }) => None,
            Err(error) => return Err(error.into()),
        };
        let mut versions: Vec<String> = fs::read_dir(layout::versions_dir(&self.paths))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                    .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        versions.sort();
        let running_dir = running_bundle_dir(&self.paths);
        let restart_required = match (&running_dir, entry) {
            (Some(running), Some(entry)) => *running != entry.dir,
            _ => false,
        };
        Ok(UpdateStatus {
            target: self.target,
            current: entry.map(|entry| entry.current()),
            previous: entry.and_then(|entry| entry.previous.clone()),
            integrity: integrity_report(
                &self.paths,
                self.target,
                &self.config.trust,
                VerifyScope::Critical,
                false,
            ),
            pending: journal
                .as_ref()
                .is_some_and(|journal| !journal.state.is_terminal()),
            journal: journal.map(JournalSummary::from),
            versions,
            running_dir,
            restart_required,
        })
    }
}
