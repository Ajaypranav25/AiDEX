# aidex-drive

Owns the portable drive contract: the root-level `aidex-drive.json` manifest (schema v2, with an explicit migration from v1), safe `init`/`status` operations, and `DrivePaths`, the single resolver services use to find their drive directories. The layout, directory meanings, and migration procedure are specified in [`portable/layouts/README.md`](../../portable/layouts/README.md).

## API

- `init(root)` validates a removable drive root, migrates a valid v1 manifest, writes a missing manifest, and creates every declared directory. `init_with(root, InitOptions { require_removable: false })` does the same for tests and explicitly chosen developer folders; AiDEX never skips the removable check implicitly. Both still require 10 MiB free.
- `status(root)` reports root, writability, free space, and schema version without changing anything; `status_with(root, StatusOptions { require_removable: false })` does the same for an explicitly chosen developer folder (frontends use it to report health without treating the folder as removable media). A v1 drive succeeds; `DriveStatus::migration_required()` and `migration_advice()` say that `aidex init --drive <root>` migrates it.
- `DrivePaths::open(root)` reads a schema-v2 manifest without mutation and returns `DriveError::MigrationRequired` for v1. It does not apply the removable check. `DrivePaths::from_layout(root, layout)` builds paths without I/O. There is one accessor per layout entry (`runtime()`, `models()`, `agents()`, `skills()`, `mcp()`, `plugins()`, `policies()`, `workspaces()`, `cache()`, `tmp()`, `logs()`, `app()`, `manifests()`, `vault()`, `identity()`, `sessions()`, `config()`, `memory()`, `index()`), plus `runtime_state()` (`.aidex/runtime`) and `platform_runtime()` (`runtime/<os>-<arch>`, or `None` on unnamed platforms).
- `DrivePaths::ensure_dir(dir)` creates a directory only when it is a declared directory, `.aidex/runtime`, or beneath one of them.
- `list_volumes()` lists mounted volumes as `VolumeInfo { root, label, filesystem, total_bytes, free_bytes, removable: RemovableStatus, system, has_aidex_manifest, schema_version }`; `volume_for(path)` returns the volume holding a path. `RemovableStatus` is `Removable` (removable media), `UsbFixed` (a fixed disk on the USB bus, such as a portable SSD), `Fixed`, or `Unknown`; `VolumeInfo::is_eligible()` is true for the first two when the volume is not the system volume. The removable check in `init`/`status` uses the same classification. Discovery is read-only: Windows runs one PowerShell query (`Get-Volume` joined with `Get-Partition | Get-Disk` bus type; a filter letter is passed through an environment variable), Linux reads `/proc/mounts`, sysfs (`removable`, a `/usb` device path), `/dev/disk/by-label`, and `df -Pk` over the listed mounts, and macOS reads `df -Pkl` and `diskutil info`.
- `StoredManifest::from_json` decodes v1 or v2; `DriveManifest::from_json` accepts only the current schema. `DriveError::code()` maps every error to a stable `aidex_core::ServiceErrorCode`.

## Guarantees

- Manifests are closed schemas. Malformed, unknown-version, and invalid manifests are reported and never rewritten.
- Layout paths are relative, traversal-free, and unique (compared case-insensitively). Reserved metadata names cannot be claimed.
- v1 → v2 migration keeps the original bytes in `aidex-drive.v1.json`, installs the new manifest by fsync and atomic rename, verifies it, and can be retried after an interruption at any step. A conflicting backup aborts with no changes.
- Directory creation (in `init` and `ensure_dir`) refuses existing symbolic links, Windows junctions, and non-directories below the root.

## Limitations

- Accessors are lexical. `ensure_dir` expects absolute paths spelled under `DrivePaths::root()` (on Windows that root is canonical, with a `\\?\` prefix), so derive paths from the accessors.
- The root itself is trusted after canonicalization; only components below it are checked for links. Checks are not atomic with later file opens, so a process that can rewrite the drive concurrently could still swap a directory for a link.
- Removable-media detection is heuristic: Windows drive type plus disk bus type, Linux sysfs `removable` plus a USB device path, macOS `diskutil` removable media plus protocol. USB-attached fixed disks are accepted (portable SSDs report themselves as fixed); internal SATA/NVMe disks, unclassified volumes, and the system volume are refused. Thunderbolt enclosures are classified as fixed. Linux and macOS volume listing are best effort and only parser-tested on CI. Windows queries shell out to PowerShell.
- Durability of rename and directory fsync on exFAT/FAT32 is best effort and has not been verified on real removable media.
- Nothing here encrypts data; `private/` directories rely on the owning services for protection.
