//! Portable-drive path validation, the schema-v1 manifest, and safe drive operations.
//!
//! The root-level `aidex-drive.json` file is the sole drive manifest. Initialization
//! creates only manifest-declared directories; status never repairs missing, malformed,
//! or unsupported manifest state. Relative paths are validated before filesystem use.

mod error;
mod manifest;
mod ops;
mod path;
mod platform;

pub use error::DriveError;
pub use manifest::{
    CURRENT_SCHEMA_VERSION, DriveLayout, DriveManifest, MANIFEST_FILE_NAME, ManifestError,
};
pub use ops::{DriveStatus, init, resolve_root, status};
pub use path::{PathError, RelativePath};
pub use platform::is_windows_drive_root;
