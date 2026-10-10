# Portable drive contract

## Recognition and initialization boundary

An AiDEX drive is recognized only when `aidex-drive.json` exists at its root and parses as a supported `DriveManifest`. A directory name, volume label, or presence of `runtime/` alone is not proof of an AiDEX drive. Unknown, malformed, or unsupported manifests are reported without rewriting them.

Ordinary initialization may create AiDEX-owned directories and write the manifest after validating the target. It MUST NOT format, repartition, delete, or rewrite unrelated user files. An existing manifest is read and validated before any write; initialization is idempotent.

## Schema v2 layout

The drive crate writes schema version 2 and reads schema versions 1 and 2. Both schemas are closed: unknown fields are rejected, and every v2 field is required. All layout paths are slash-separated paths relative to the drive root; absolute roots, parent traversal, empty paths, and unsupported components (such as `C:`) are rejected. Two entries may not name the same directory; this comparison ignores letter case and trailing dots or spaces per segment, because exFAT, FAT32, and NTFS treat those spellings as one directory. One entry may be nested beneath another. Entries may not claim root names reserved for drive metadata: `aidex-drive.json`, `aidex-drive.v1.json`, `aidex-drive.json.v2-tmp`, or `.aidex`.

| Manifest field | Default path | Meaning |
| --- | --- | --- |
| `public_runtime` | `runtime` | Portable runtime bundles, one `<os>-<arch>` directory per host (`windows-x64`, `linux-x64`, `linux-arm64`, `macos-arm64`, `macos-x64`). |
| `public_models` | `models` | Model weights and the model registry. Public weights are not encrypted by default. |
| `public_agents` | `agents` | Agent definitions and adapter assets. |
| `public_skills` | `skills` | Portable skills. |
| `public_mcp` | `mcp` | MCP server definitions. |
| `public_plugins` | `plugins` | Installed plugins. |
| `public_policies` | `policies` | User-editable policy files. |
| `workspaces` | `workspaces` | Drive-resident and ephemeral workspaces. |
| `cache` | `cache` | Rebuildable cache data; safe to clear. |
| `tmp` | `tmp` | Short-lived scratch files. |
| `logs` | `logs` | Service logs. Logs must not contain secrets. |
| `app` | `app` | Application bundles and staged updates. |
| `manifests` | `manifests` | Integrity manifests such as checksums and SBOMs. |
| `private_vault` | `private/vault` | Encrypted vault storage. |
| `private_identity` | `private/identity` | Drive identity keys. |
| `private_sessions` | `private/sessions` | Durable sessions, events, and their blobs. |
| `private_config` | `private/config` | Private configuration, such as registries and the active policy profile. |
| `private_memory` | `private/memory` | Portable user memory. |
| `private_index` | `private/index` | Per-workspace search indexes. |

Application bundles inside `app/` (`current.json`, `versions/<version>-<target>/`, `launcher/<target>/`, `staging/`) and the drive-root launchers (`AiDEX.exe`, `aidex`) are specified in [`portable/packaging/README.md`](../packaging/README.md#drive-layout). The root names `AiDEX.exe`, `aidex`, and their `.previous` / `.aidex-new` siblings are written only by the bundle installer, which never replaces a file that is not an AiDEX launcher.

`public_*` and top-level operational directories hold data that may stay unencrypted. `private/` directories hold state that the owning service must protect; the layout itself does not encrypt anything. Daemon runtime metadata lives in `.aidex/runtime/`, which is not a manifest entry.

Services resolve these directories through `aidex_drive::DrivePaths`, which reads the manifest without modifying it, requires schema v2, and creates directories only through `ensure_dir`, which accepts only declared directories (or paths beneath them) and refuses symbolic links, junctions, and non-directories.

## Migration policy and v1 → v2 procedure

A migration must be an explicit, version-to-version transformation, preserve the original until the transformed manifest is verified, and report unsupported versions without mutation. Secret data must remain encrypted during migration. Manifests with newer, unknown, or malformed schemas are never guessed, upgraded, or rewritten.

Schema v1 declared only `public_runtime`, `public_models`, `private_vault`, `private_identity`, `private_sessions`, and `private_config`. `aidex status` succeeds on a v1 drive, reports schema version 1, and states that migration is available. Services that open the drive through `DrivePaths` refuse v1 with advice to run `aidex init --drive <root>`, which performs the migration:

1. Transform in memory: keep all six v1 paths unchanged and add the v2 entries at their defaults, then validate the result. If a v1 path collides with a new default (for example, `public_runtime` set to `cache`), migration stops before writing anything.
2. Write the exact original manifest bytes to `aidex-drive.v1.json` with create-new semantics. If that file already exists with identical bytes, a previous attempt reached this step and migration continues. If it exists with different content (or is not a regular file), migration aborts and changes nothing; move the file aside after confirming it is not needed, then retry.
3. Write the v2 manifest to `aidex-drive.json.v2-tmp` and fsync it. A stale staging file from an interrupted attempt is replaced.
4. Confirm that `aidex-drive.json` still holds the original bytes, then atomically rename the staging file over it.
5. Re-read and verify the installed manifest, then create the declared directories.

Each step is safe to repeat after an interruption, and the backup is always kept. No directory is moved or renamed and no data file is touched, so existing v1 data stays where the v1 layout put it. One limitation: if power is lost while the small backup file itself is being written, the retry reports a conflict because the partial backup differs from the original; the original manifest is still intact, so removing the partial backup and retrying is safe.

## Platform and filesystem support status

| Operating system | Filesystem | Status |
| --- | --- | --- |
| Windows x64 | NTFS / exFAT / FAT32 removable volumes | Planned product targets; unit tests exercise manifest, migration, and directory semantics in temporary NTFS directories. No removable-volume integration test has been run. |
| Linux x64 / arm64 | ext4 / exFAT removable volumes | Planned; not tested. |
| macOS x64 / arm64 | APFS / exFAT removable volumes | Planned; not tested. |

The manifest contract is filesystem-neutral. This matrix does not claim that path semantics, atomic writes, permissions, eject behavior, or durability have been verified on a listed filesystem; those require platform integration tests before a support claim. In particular, rename atomicity and directory fsync are best-effort on FAT-family filesystems.
