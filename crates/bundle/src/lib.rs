//! Portable AiDEX bundles: the closed-schema `bundle.json` manifest, the drive's atomic
//! `app/current.json` pointer, integrity verification, and crash-safe installation of
//! a bundle onto an initialized drive.
//!
//! The packager writes and signs bundles (`bundle.sig`, Ed25519 over the exact
//! `bundle.json` bytes), the drive-root launcher verifies the active bundle and its
//! signature against a compiled-in [`TrustStore`] before running it
//! ([`resolve_active`]), and the updater builds staged updates, its journal
//! ([`UpdateJournal`]), and rollback on [`steps`] and [`CurrentPointer`]. Archive
//! writing and extraction are behind the `archive` feature so the launcher stays small.

mod active;
#[cfg(feature = "archive")]
pub mod archive;
mod catalog;
mod digest;
mod entry;
mod error;
mod fsutil;
mod install;
mod jobs;
mod journal;
pub mod layout;
mod manifest;
mod path;
mod pointer;
mod signature;
mod sums;
mod target;
mod verify;

pub use active::{ActiveBundle, resolve_active};
pub use catalog::{BundleCatalog, desktop_catalog};
pub use digest::{is_sha256_hex, sha256_file, sha256_hex};
pub use entry::{LAUNCHER_MARKER, is_aidex_launcher, unix_entry_script};
pub use error::{BundleError, BundleErrorKind, DocumentError, IntegrityProblem, SignatureError};
pub use fsutil::{stale_temporaries, sync_dir, write_atomic, write_atomic_with};
pub use install::{
    InstallReport, InstallSource, InstallStep, LauncherPlacement, PlacementAction, install,
    install_signed, install_with, steps,
};
pub use jobs::{InstallJob, InstallJobs, JobState};
pub use journal::{
    JOURNAL_FILE_NAME, JOURNAL_SCHEMA_VERSION, JournalOperation, JournalState, UPDATE_ID_PREFIX,
    UpdateJournal, journal_path,
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
pub use signature::{
    BUILD_TRUSTED_KEYS_ENV, BundleSignature, DEV_TRUSTED_KEYS_ENV, KEY_ALGORITHM, KeyPair,
    KeySource, PublicKeyFile, REVOCATIONS_FILE_NAME, Revocation, RevocationList, SIGNATURE_CONTEXT,
    SIGNATURE_FILE_NAME, SIGNATURE_SCHEMA_VERSION, SignaturePolicy, SignatureState, SigningKeyFile,
    TrustStore, TrustedKey, key_id_for, read_signature, revocations_path, sign_manifest,
    signed_message, verify_signature, write_signature,
};
pub use sums::{SHA256SUMS_FILE_NAME, Sha256Sums};
pub use target::{Target, UnknownTarget};
pub use verify::{VerifiedBundle, VerifyScope, read_manifest, read_manifest_bytes, verify_bundle};
