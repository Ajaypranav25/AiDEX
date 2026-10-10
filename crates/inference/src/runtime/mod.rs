//! Pinned llama.cpp runtime bundles (spec §10.2, §67): the catalog, installs on
//! the drive, and verification before launch.

mod catalog;
mod install;
mod installed;

pub use catalog::{
    ArchiveFormat, BUILTIN_CATALOG, CATALOG_VERSION, CatalogEntry, RuntimeCatalog, RuntimeVariant,
};
pub use install::{InstallPhase, install_from_catalog, install_runtime, install_runtime_with};
pub use installed::{
    ENGINE_DIR, InstalledListing, InstalledRuntime, MANIFEST_FILE, MANIFEST_VERSION, ManifestFile,
    RuntimeManifest, RuntimeProblem, RuntimeVerification, list_installed, tree_sha256,
};
