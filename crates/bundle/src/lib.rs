//! Portable AiDEX bundles: the closed-schema `bundle.json` manifest, the drive's atomic
//! `app/current.json` pointer, integrity verification, and crash-safe installation of
//! a bundle onto an initialized drive.
//!
//! The packager writes bundles, the drive-root launcher verifies the active bundle
//! before running it ([`resolve_active`]), and the updater builds staged updates and
//! rollback on [`install_with`] and [`CurrentPointer`]. Archive writing and extraction
//! are behind the `archive` feature so the launcher stays small.

mod active;
#[cfg(feature = "archive")]
pub mod archive;
mod digest;
mod entry;
mod error;
mod fsutil;
mod install;
pub mod layout;
mod manifest;
mod path;
mod pointer;
mod sums;
mod target;
mod verify;

pub use active::{ActiveBundle, resolve_active};
pub use digest::{is_sha256_hex, sha256_file, sha256_hex};
pub use entry::{LAUNCHER_MARKER, is_aidex_launcher, unix_entry_script};
pub use error::{BundleError, BundleErrorKind, DocumentError, IntegrityProblem};
pub use install::{
    InstallReport, InstallSource, InstallStep, LauncherPlacement, PlacementAction, install,
    install_with,
};
pub use manifest::{
    BundleFile, BundleManifest, FileRole, LicenseEntry, MANIFEST_FILE_NAME,
    MANIFEST_SCHEMA_VERSION, MAX_MANIFEST_BYTES, PRODUCT_NAME, RUNTIME_STATE_PATH, StatePaths,
};
pub use path::{
    parse_pointer_dir, pointer_dir, validate_bundle_path, validate_git_commit, validate_version,
    version_dir_name,
};
pub use pointer::{
    CurrentPointer, POINTER_FILE_NAME, POINTER_SCHEMA_VERSION, TargetPointer, VersionRef,
};
pub use sums::{SHA256SUMS_FILE_NAME, Sha256Sums};
pub use target::{Target, UnknownTarget};
pub use verify::{VerifiedBundle, VerifyScope, read_manifest, read_manifest_bytes, verify_bundle};
