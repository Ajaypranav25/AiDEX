# Portable drive contract

## Recognition and initialization boundary

An AiDEX drive is recognized only when `aidex-drive.json` exists at its root and parses as a supported `DriveManifest`. A directory name, volume label, or presence of `runtime/` alone is not proof of an AiDEX drive. Unknown, malformed, or unsupported manifests are reported without rewriting them.

Ordinary initialization may create AiDEX-owned directories and write the manifest after validating the target. It MUST NOT format, repartition, delete, or rewrite unrelated user files. An existing manifest is read and validated before any write; initialization is idempotent.

## Schema and migration policy

The drive crate currently reads and writes schema version 1. Its default layout separates public `runtime/` and `models/` from private `private/vault/`, `private/identity/`, `private/sessions/`, and `private/config/` directories. All manifest paths are slash-separated, relative paths; absolute roots, parent traversal, empty paths, and unsupported components are rejected.

No migration is currently implemented. A manifest with an older or newer schema is not guessed, silently upgraded, or rewritten. A future migration must be an explicit, version-to-version transformation, preserve the original until the transformed manifest and data are verified, and report unsupported versions without mutation. Secret data must remain encrypted during migration.

## Platform and filesystem support status

| Operating system | Filesystem | Status |
| --- | --- | --- |
| Windows x64 | NTFS / exFAT / FAT32 removable volumes | Planned product targets; current unit tests exercise Windows path semantics only. No removable-volume integration test has been run. |
| Linux x64 / arm64 | ext4 / exFAT removable volumes | Planned; not tested. |
| macOS x64 / arm64 | APFS / exFAT removable volumes | Planned; not tested. |

The manifest contract is filesystem-neutral. This matrix does not claim that path semantics, atomic writes, permissions, eject behavior, or durability have been verified on a listed filesystem; those require platform integration tests before a support claim.