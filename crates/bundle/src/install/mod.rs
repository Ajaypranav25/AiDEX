//! Atomic installation of a bundle onto an initialized drive.
//!
//! ```text
//! stage          copy (or extract) into app/staging/<id>/ on the drive itself
//! verify         parse bundle.json, preflight root launchers, hash every file
//! commit         rename the staged bundle to app/versions/<version>-<target>/
//! place_launcher write AiDEX.exe (Windows) or app/launcher/<t>/aidex-launcher + ./aidex (Unix)
//! switch_pointer atomically replace app/current.json (temporary file + fsync + rename)
//! cleanup        remove the staging directory
//! ```
//!
//! Until `switch_pointer` completes the previous `app/current.json` is untouched, so an
//! interruption at any earlier point leaves the previously active bundle runnable.
//! [`install_signed`] additionally checks `bundle.sig` against a [`SignaturePolicy`]
//! during `verify`; [`install`] and [`install_with`] check integrity only and leave
//! the trust decision to the launcher, which refuses unsigned bundles by default.
//! Version directories are never modified once committed; reinstalling the same build
//! reuses a verified copy, and a different build of the same version (or a repair of a
//! corrupted copy) is committed under a digest-suffixed name instead of overwriting.

mod place;
mod stage;
pub mod steps;

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use aidex_core::EntityId;
use aidex_drive::{CURRENT_SCHEMA_VERSION, DrivePaths};
use serde::Serialize;

pub use place::{LauncherPlacement, PlacementAction};

use crate::{
    BundleError, CurrentPointer, SignaturePolicy, SignatureState, Target, VerifyScope, VersionRef,
    digest::sha256_hex,
    layout::{STAGING_DIR, VERSIONS_DIR, pointer_path},
    path::pointer_dir,
    signature::verify_signature,
    verify::{read_manifest, read_manifest_bytes, verify_bundle},
};

/// Where a bundle comes from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallSource {
    /// An unpacked bundle directory (the one containing `bundle.json`).
    Directory(PathBuf),
    /// A `.zip` or `.tar.gz` produced by the packager.
    #[cfg(feature = "archive")]
    Archive(PathBuf),
}

impl InstallSource {
    /// A directory, or (with the `archive` feature) a `.zip` / `.tar.gz` file.
    pub fn from_path(path: &Path) -> Result<Self, BundleError> {
        let metadata = fs::metadata(path).map_err(|error| {
            BundleError::InvalidSource(format!("`{}`: {error}", path.display()))
        })?;
        if metadata.is_dir() {
            return Ok(Self::Directory(path.to_path_buf()));
        }
        #[cfg(feature = "archive")]
        if crate::archive::ArchiveFormat::detect(path).is_some() {
            return Ok(Self::Archive(path.to_path_buf()));
        }
        Err(BundleError::InvalidSource(format!(
            "`{}` is neither a bundle directory nor a .zip/.tar.gz bundle archive",
            path.display()
        )))
    }
}

/// Install steps, reported to an observer before each one starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStep {
    Stage,
    Verify,
    Commit,
    PlaceLauncher,
    SwitchPointer,
    Cleanup,
}

impl InstallStep {
    pub const ALL: [InstallStep; 6] = [
        InstallStep::Stage,
        InstallStep::Verify,
        InstallStep::Commit,
        InstallStep::PlaceLauncher,
        InstallStep::SwitchPointer,
        InstallStep::Cleanup,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stage => "stage",
            Self::Verify => "verify",
            Self::Commit => "commit",
            Self::PlaceLauncher => "place_launcher",
            Self::SwitchPointer => "switch_pointer",
            Self::Cleanup => "cleanup",
        }
    }
}

impl fmt::Display for InstallStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What an install did.
#[derive(Clone, Debug, Serialize)]
pub struct InstallReport {
    pub drive_root: PathBuf,
    pub target: Target,
    pub version: String,
    /// `versions/<name>` now selected by `app/current.json`.
    pub dir: String,
    pub manifest_sha256: String,
    /// A verified identical copy was already installed and was reused.
    pub reused_existing: bool,
    /// The bundle that was active for this target before (rollback candidate).
    pub previous: Option<VersionRef>,
    pub launchers: Vec<LauncherPlacement>,
    /// How the signature was accepted ([`install_signed`] only).
    pub signature: Option<SignatureState>,
}

/// Install `source` onto the initialized drive at `drive_root` (integrity only).
pub fn install(source: &InstallSource, drive_root: &Path) -> Result<InstallReport, BundleError> {
    install_with(source, drive_root, &mut |_| Ok(()))
}

/// [`install`] with an observer that is called before each [`InstallStep`]; returning
/// an error stops the install there (used for progress, cancellation, and fault tests).
pub fn install_with(
    source: &InstallSource,
    drive_root: &Path,
    observer: &mut dyn FnMut(InstallStep) -> Result<(), BundleError>,
) -> Result<InstallReport, BundleError> {
    install_inner(source, drive_root, None, observer)
}

/// [`install_with`] that also requires `bundle.sig` to satisfy `policy` before
/// anything is committed.
pub fn install_signed(
    source: &InstallSource,
    drive_root: &Path,
    policy: &SignaturePolicy,
    observer: &mut dyn FnMut(InstallStep) -> Result<(), BundleError>,
) -> Result<InstallReport, BundleError> {
    install_inner(source, drive_root, Some(policy), observer)
}

fn install_inner(
    source: &InstallSource,
    drive_root: &Path,
    policy: Option<&SignaturePolicy>,
    observer: &mut dyn FnMut(InstallStep) -> Result<(), BundleError>,
) -> Result<InstallReport, BundleError> {
    let paths = DrivePaths::open(drive_root)?;
    let app = paths.ensure_dir(&paths.app())?;
    let pointer_file = pointer_path(&paths);
    // Refuse an unreadable pointer before writing anything.
    CurrentPointer::load(&pointer_file)?;
    let staging_root = paths.ensure_dir(&app.join(STAGING_DIR))?;
    let versions = paths.ensure_dir(&app.join(VERSIONS_DIR))?;
    let id = EntityId::generate("stage").map_err(|error| {
        BundleError::io(
            "cannot name a staging directory",
            std::io::Error::other(error.to_string()),
        )
    })?;
    let staging = staging_root.join(id.as_str());
    fs::create_dir(&staging).map_err(|error| {
        BundleError::io(format!("cannot create `{}`", staging.display()), error)
    })?;
    let result = run(
        &paths,
        source,
        &staging,
        &versions,
        &pointer_file,
        policy,
        observer,
    );
    // The staging directory is ours alone; remove whatever is left of it.
    let _ = fs::remove_dir_all(&staging);
    result
}

fn run(
    paths: &DrivePaths,
    source: &InstallSource,
    staging: &Path,
    versions: &Path,
    pointer_file: &Path,
    policy: Option<&SignaturePolicy>,
    observer: &mut dyn FnMut(InstallStep) -> Result<(), BundleError>,
) -> Result<InstallReport, BundleError> {
    observer(InstallStep::Stage)?;
    let staged = stage::stage(source, staging)?;
    let (manifest, _) = read_manifest(&staged)?;
    place::preflight(paths, &staged, &manifest)?;

    observer(InstallStep::Verify)?;
    stage::apply_modes(&staged, &manifest)?;
    let bundle = verify_bundle(&staged, VerifyScope::All)?;
    let signature = match policy {
        Some(policy) => {
            let bytes = read_manifest_bytes(&staged)?;
            if sha256_hex(&bytes) != bundle.manifest_sha256 {
                return Err(BundleError::InvalidSource(
                    "the staged bundle.json changed during verification".into(),
                ));
            }
            Some(verify_signature(
                &staged,
                &bytes,
                &bundle.manifest.created_at,
                policy,
            )?)
        }
        None => None,
    };
    if bundle.manifest.min_drive_schema > CURRENT_SCHEMA_VERSION {
        return Err(BundleError::DriveSchemaTooOld {
            required: bundle.manifest.min_drive_schema,
            found: CURRENT_SCHEMA_VERSION,
        });
    }
    if bundle.manifest.state.host_extraction {
        return Err(BundleError::HostExtractionUnsupported);
    }

    observer(InstallStep::Commit)?;
    let plan = steps::plan_commit(versions, &bundle)?;
    steps::execute_commit(&staged, versions, &plan)?;
    let (name, reused_existing) = (plan.name().to_owned(), plan.is_reuse());

    observer(InstallStep::PlaceLauncher)?;
    let launchers = place::place_launchers(paths, &versions.join(&name), &bundle.manifest)?;

    observer(InstallStep::SwitchPointer)?;
    let mut pointer = CurrentPointer::load(pointer_file)?.unwrap_or_default();
    let target = bundle.manifest.target;
    let previous = pointer.get(target).map(|entry| entry.current());
    let next = VersionRef {
        version: bundle.manifest.version.clone(),
        dir: pointer_dir(&name),
        manifest_sha256: bundle.manifest_sha256.clone(),
    };
    pointer.switch(target, next.clone());
    pointer.store(pointer_file)?;

    observer(InstallStep::Cleanup)?;
    Ok(InstallReport {
        drive_root: paths.root().to_path_buf(),
        target,
        version: next.version,
        dir: next.dir.clone(),
        manifest_sha256: next.manifest_sha256,
        reused_existing,
        previous: previous.filter(|previous| previous.dir != next.dir),
        launchers,
        signature,
    })
}
