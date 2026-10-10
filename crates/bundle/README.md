# aidex-bundle

The portable bundle contract shared by the packager, the drive-root launcher, and the updater (#27): the closed-schema `bundle.json` manifest, the drive's atomic `app/current.json` pointer, integrity verification, deterministic archives, and crash-safe installation onto an initialized drive. Schemas, layout, launcher behaviour, and limits are specified in [`portable/packaging/README.md`](../../portable/packaging/README.md).

## API

- `Target` (`windows-x64`, `linux-x64`, `linux-arm64`, `macos-arm64`, `macos-x64`; serde/`FromStr` use these names): `host()`, `executable_name(stem)`, `default_rust_triple()`.
- `BundleManifest { schema_version, product, version, git_commit, source_dirty, target, rust_target, rustc, created_at, min_drive_schema, state: StatePaths, files: Vec<BundleFile>, licenses: Vec<LicenseEntry> }` with `from_json(&[u8])`, `to_json()`, `validate()`, `normalize()`, `file(FileRole)`, `find(path)`, `critical_files()`. `BundleFile { path, sha256, size, executable, role }` and `BundleFile::describe(bundle_dir, path, role, executable)`; `FileRole::{Launcher, Cli, Daemon, Helper, Desktop, Installer, Runtime, Doc}` with `is_critical()`; `LicenseEntry { component, version, spdx, source, path }`; `StatePaths { drive_manifest, runtime_state, layout, host_extraction }` and `StatePaths::current()`.
- `CurrentPointer { schema_version, targets: BTreeMap<String, TargetPointer> }`, `TargetPointer { version, dir, manifest_sha256, previous: Option<VersionRef> }`, `VersionRef { version, dir, manifest_sha256 }`: `load(path) -> Result<Option<_>>`, `store(path)` (temporary file + fsync + rename), `get(Target)`, `switch(Target, VersionRef)` (the old entry becomes `previous`), `installed_targets()`.
- `verify_bundle(dir, VerifyScope::{Critical, All}) -> VerifiedBundle { dir, manifest, manifest_sha256, scope, verified_files }`, `read_manifest(dir)`, `read_manifest_bytes(dir)`.
- `resolve_active(&DrivePaths, Target, VerifyScope) -> ActiveBundle { target, pointer, bundle }`: pointer → pinned manifest → target/version/drive-schema checks → file verification. This is exactly what the launcher runs before executing anything.
- `install(&InstallSource, drive_root)` and `install_with(.., observer: &mut dyn FnMut(InstallStep) -> Result<(), BundleError>)` → `InstallReport { drive_root, target, version, dir, manifest_sha256, reused_existing, previous, launchers: Vec<LauncherPlacement> }`. `InstallSource::{Directory, Archive}` (`from_path` detects); `InstallStep::{Stage, Verify, Commit, PlaceLauncher, SwitchPointer, Cleanup}`; the observer runs before each step and can stop the install (progress, cancellation, fault-injection tests).
- `LAUNCHER_MARKER`, `is_aidex_launcher(path)`, `unix_entry_script(app_relative)`; `layout::*` names and path helpers (`pointer_path`, `versions_dir`, `launcher_path`, `bin_path`, ...).
- `Sha256Sums` (`parse`, `load`, `get`, `set`, `render`, `store`) for `sha256sum`-compatible checksum files; `sha256_hex`, `sha256_file`, `is_sha256_hex`.
- Feature `archive` (packager and updater; the launcher does not enable it): `archive::{write_archive, extract_archive, ExtractLimits, ArchiveFormat, archive_file_name, bundle_root_name}`.
- `BundleError` with `kind() -> BundleErrorKind::{Drive, NotInstalled, Incompatible, Integrity, Occupied, InvalidInput, Aborted, Io}` and `code() -> aidex_core::ServiceErrorCode`; `DocumentError::{UnsupportedSchema, Malformed, Invalid}`; `IntegrityProblem::{Missing, NotRegularFile, Link, SizeMismatch, DigestMismatch, Unlisted, ManifestPin}`.

## Guarantees

- Manifests and pointers are closed schemas; a different `schema_version` is reported as unsupported before any field is interpreted. Paths are portable across NTFS/exFAT/FAT32/ext4/APFS, unique case-insensitively, and cannot escape the bundle; pointer directories cannot escape `app/versions/`.
- Verification refuses symbolic links and junctions anywhere on a file's path, size or SHA-256 mismatches, missing files, and unlisted files (in `bin/` for the critical scope; anywhere for the full scope). `resolve_active` compares `bundle.json` with its pinned digest before parsing it.
- Installation stages on the drive, verifies everything before committing, never modifies a committed version directory, replaces only files carrying the launcher marker (keeping `<name>.previous`), and leaves `app/current.json` untouched until the final atomic switch. Every interruption point is covered by tests.
- Archive writing is deterministic; extraction accepts only regular files and directories under one top-level directory, with entry-count and size limits.

## Limitations

- Integrity, not authenticity: nothing is signed yet (#27), and a check-then-exec race with a concurrent writer of the drive is not prevented.
- No locking between concurrent installs on one drive, and no pruning of old version directories.
- Directory fsync after renames is a no-op on Windows and best effort on FAT-family filesystems.
- Unix modes are applied from the manifest; on filesystems without permission bits (FAT, exFAT) the mount options decide executability.
