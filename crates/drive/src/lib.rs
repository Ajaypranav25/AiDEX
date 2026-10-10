//! Portable-drive path validation, the schema-v2 manifest with explicit v1 migration,
//! safe drive operations, and service directory resolution.
//!
//! The root-level `aidex-drive.json` file is the sole drive manifest. Initialization
//! creates only manifest-declared directories and migrates a valid schema-v1 manifest
//! only through an explicit, backed-up, verified transform; status and [`DrivePaths`]
//! never repair or rewrite missing, malformed, or unsupported manifest state. Relative
//! paths are validated before filesystem use.

mod error;
mod layout;
mod manifest;
mod migration;
mod ops;
mod path;
mod paths;
mod platform;

pub use error::DriveError;
pub use layout::{DriveLayout, DriveLayoutV1};
pub use manifest::{
    CURRENT_SCHEMA_VERSION, DriveManifest, DriveManifestV1, MANIFEST_FILE_NAME, ManifestError,
    StoredManifest, V1_SCHEMA_VERSION,
};
pub use migration::V1_BACKUP_FILE_NAME;
pub use ops::{DriveStatus, init, resolve_root, status};
pub use path::{PathError, RelativePath};
pub use paths::{DrivePaths, platform_runtime_name};
pub use platform::is_windows_drive_root;
