# Portable packaging

AiDEX runs from the drive: a bundle of native binaries is installed into the drive's `app/` directory and started through a small launcher at the drive root. Nothing is installed on, or extracted to, the host.

| Crate | Role |
| --- | --- |
| [`aidex-bundle`](../../crates/bundle/README.md) | Shared contract: the `bundle.json` manifest, the `app/current.json` pointer, verification, deterministic archives, and atomic installation. The launcher, the packager, and the updater (#27) all use it. |
| [`aidex-launcher`](../../crates/launcher/README.md) | The drive-root launcher (`AiDEX.exe`, or `./aidex` → `app/launcher/<target>/aidex-launcher`). Verifies before it runs anything. |
| [`aidex-packager`](../../crates/packager/README.md) | Developer/CI tool: builds release binaries into a bundle, writes the archive and `SHA256SUMS`, installs bundles onto drives, and smoke-tests them. Never shipped in a bundle. |

The packager is its own crate rather than a launcher sub-binary because it drives `cargo` and `git`, reads `cargo metadata`, and writes archives — none of which belongs in the binary that every launch executes. Installation lives in `aidex-bundle` so the updater can stage, verify, and switch bundles with the same code.

## Build a bundle

```sh
cargo run --locked -p aidex-packager -- build --target windows-x64 --out dist
```

`build` runs `cargo build --release --locked --target <triple>` for exactly the shipped binaries (`aidex` from `aidex-cli`, `aidexd`, `aidex-sandbox-exec` from `aidex-sandbox`, and `aidex-launcher`) into `target/packager/`, then:

1. collects license metadata for every crate linked into them (see [Licenses](#licenses));
2. assembles `dist/.aidex-<version>-<target>.staging-<pid>/`, writes `bundle.json`, verifies every file against it, and renames it to `dist/aidex-<version>-<target>/` (replacing only a previous packager output);
3. writes the deterministic archive `dist/aidex-<version>-<target>.zip` (Windows) or `.tar.gz` (Linux/macOS);
4. adds or replaces the archive's line in `dist/SHA256SUMS` (`sha256sum -c SHA256SUMS` verifies it).

Each target is built on its own operating system; cross-compiling is not supported or tested. The default triples are `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-musl` (static, so the binaries do not depend on the host's glibc; needs `rustup target add x86_64-unknown-linux-musl` and `musl-tools` for the bundled SQLite), and `aarch64-apple-darwin`; `--triple` overrides it and is recorded in the manifest.

Packaging build settings, applied through the environment of that one `cargo build` (they replace any `RUSTFLAGS` you have set):

| Setting | Why |
| --- | --- |
| `-C target-feature=+crt-static` (Windows) | No dependency on the Visual C++ runtime being installed on the host. |
| `-C link-arg=/Brepro` (Windows) | Deterministic PE timestamps. |
| `--remap-path-prefix` for the workspace and `CARGO_HOME` | No local paths (user names) in panic messages; fewer machine-specific bytes. |
| `CARGO_PROFILE_RELEASE_STRIP=symbols` | Smaller binaries. |
| `SOURCE_DATE_EPOCH` | Commit time (or your `SOURCE_DATE_EPOCH`), also used as `created_at` and as every archive entry's timestamp. |
| `MACOSX_DEPLOYMENT_TARGET=11.0` (macOS) | Runs on macOS 11 and later. |

Optional components: `--desktop <path>` and `--installer <path>` copy a *prebuilt* Tauri app (a file, or a directory such as `AiDEX.app`) under `desktop/` or `installer/` with roles `desktop` / `installer`. The packager never builds the Tauri apps, and nothing in the CLI bundle depends on them. Symbolic links are refused, so macOS app bundles that contain framework links cannot be packaged yet.

## Install a bundle onto a drive

```sh
aidex init --drive H:\                     # once: creates aidex-drive.json and the layout
cargo run -p aidex-packager -- install --bundle dist/aidex-0.1.0-windows-x64.zip --drive H:\
H:\AiDEX.exe doctor
```

`--bundle` takes an archive or an unpacked bundle directory. For a developer folder or a test directory that is not removable media, `--init-dev-drive` initializes it first with the removable-media check skipped (the same as `aidex_drive::init_with(.., InitOptions { require_removable: false })`); it is never implied.

Install steps (each is reported to `install_with`'s observer, which can stop the install before it starts):

| Step | What happens |
| --- | --- |
| `stage` | Copy `bundle.json` and exactly the files it lists (directory source) or extract the archive (bounded, links refused) into `app/staging/<id>/` on the drive itself, so the commit is a same-volume rename. |
| `verify` | Parse and validate `bundle.json`; check that the launcher carries the AiDEX launcher marker and that every launcher destination is absent or an AiDEX launcher; apply executable modes; hash every listed file and refuse unlisted files; refuse bundles that need a newer drive schema or host extraction. |
| `commit` | Rename the staged bundle to `app/versions/<version>-<target>/`. If that directory holds a verified copy of the same `bundle.json`, reuse it. If it holds a different build (or a damaged copy), use `<version>-<target>-<first 12 hex of the bundle.json sha256>` instead. Committed directories are never modified. |
| `place_launcher` | Windows targets: copy `bin/aidex-launcher.exe` to `<drive>/AiDEX.exe`. Unix targets: copy `bin/aidex-launcher` to `app/launcher/<target>/aidex-launcher` and write the `<drive>/aidex` entry script. Each file is written as `<name>.aidex-new`, read back, and renamed into place; a replaced launcher is kept as `<name>.previous`. Identical files are left alone. |
| `switch_pointer` | Rewrite `app/current.json` with temporary file + fsync + rename. The target's old entry becomes `previous`. |
| `cleanup` | Remove `app/staging/<id>/`. |

Guarantees:

- Until `switch_pointer` completes, the previous `app/current.json` is byte-for-byte unchanged and the previously active bundle still verifies and runs; an interruption after `place_launcher` leaves a newer launcher in front of the old bundle, which it runs. Retrying the install completes it.
- A drive-root file is replaced only if it is a regular file containing the AiDEX launcher marker. Any other `AiDEX.exe`, `aidex`, or `*.previous` / `*.aidex-new` file makes the install fail before anything is committed.
- A tampered or incomplete source never reaches `app/versions/`; an unreadable `app/current.json` is reported and left untouched.
- Bundles for other targets can be installed from any host (for example, the Linux bundle from Windows); each target has its own pointer entry and launcher.
- The installer does not prune old version directories, and does not take a lock: run one install per drive at a time (staging directories are unique, and the pointer write is atomic last-writer-wins).

## Drive layout

```text
<drive>/
├── aidex-drive.json                  drive manifest (aidex-drive; schema v2)
├── AiDEX.exe                         Windows launcher (copy of bin/aidex-launcher.exe)
├── AiDEX.exe.previous                launcher replaced by the last install, if any
├── aidex                             POSIX sh entry for Linux and macOS
├── app/
│   ├── current.json                  active bundle per target (atomic pointer)
│   ├── versions/
│   │   └── <version>-<target>[-<digest>]/
│   │       ├── bundle.json           bundle manifest
│   │       ├── bin/aidex[.exe]                 CLI
│   │       ├── bin/aidexd[.exe]                daemon
│   │       ├── bin/aidex-sandbox-exec[.exe]    sandbox helper (next to aidexd)
│   │       ├── bin/aidex-launcher[.exe]        launcher
│   │       ├── licenses/aidex/LICENSE
│   │       ├── licenses/third-party/<crate>-<version>.txt
│   │       ├── desktop/   installer/           optional prebuilt apps
│   ├── launcher/<target>/aidex-launcher        Unix launchers (selected by ./aidex)
│   └── staging/<id>/                 installs in progress (removed when done)
└── manifests/                        reserved for checksum/SBOM copies (#27)
```

The launcher binary is `aidex-launcher` inside bundles because `AiDEX.exe` and `aidex.exe` are the same file on case-insensitive filesystems (NTFS, exFAT, FAT32, default APFS). `app` is the drive manifest's `app` path; other layout paths come from `aidex-drive.json` as described in [`portable/layouts/README.md`](../layouts/README.md).

## `bundle.json` (schema 1)

A closed schema: every object rejects unknown fields, and a document with another `schema_version` is reported as unsupported (not as malformed). Rust types: `aidex_bundle::{BundleManifest, BundleFile, FileRole, LicenseEntry, StatePaths}`.

```json
{
  "schema_version": 1,
  "product": "aidex",
  "version": "0.1.0",
  "git_commit": "<40 or 64 lowercase hex>",
  "source_dirty": false,
  "target": "windows-x64",
  "rust_target": "x86_64-pc-windows-msvc",
  "rustc": "rustc 1.95.0 (59807616e 2026-04-14)",
  "created_at": "2026-10-10T08:00:00.000Z",
  "min_drive_schema": 2,
  "state": {
    "drive_manifest": "aidex-drive.json",
    "runtime_state": ".aidex/runtime",
    "layout": { "app": "app", "cache": "cache", "private_sessions": "private/sessions", "...": "..." },
    "host_extraction": false
  },
  "files": [
    { "path": "bin/aidex.exe", "sha256": "<64 hex>", "size": 4321, "executable": true, "role": "cli" }
  ],
  "licenses": [
    { "component": "serde", "version": "1.0.229", "spdx": "MIT OR Apache-2.0",
      "source": "registry+https://github.com/rust-lang/crates.io-index",
      "path": "licenses/third-party/serde-1.0.229.txt" }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `schema_version` | Always `1` for this format. |
| `product` | Always `aidex`. |
| `version` | Product version (the `aidex-cli` package version): starts with a digit, `[0-9A-Za-z.+-]`, at most 64 bytes. |
| `git_commit`, `source_dirty` | Source commit and whether tracked or untracked files differed from it (`null` when built outside git). |
| `target` | `windows-x64`, `linux-x64`, `linux-arm64`, `macos-arm64`, or `macos-x64`. |
| `rust_target`, `rustc` | Compiler target triple and version. |
| `created_at` | RFC 3339 UTC; the commit time (or `SOURCE_DATE_EPOCH`) so rebuilds of one commit produce the same manifest. |
| `min_drive_schema` | Lowest `aidex-drive.json` schema the bundle runs on. |
| `state` | Drive-relative state paths the binaries use at build time (`aidex-drive.json`, `.aidex/runtime`, and the default layout); services still resolve real paths from the drive manifest. `host_extraction` is always `false`; launchers refuse a bundle that sets it. |
| `files` | Every file except `bundle.json`, sorted by path: bundle-relative path, lowercase-hex SHA-256, size, executable flag, role. |
| `licenses` | Every crate linked into the binaries, sorted: name, version, declared SPDX expression (or `NOASSERTION`), source (`workspace`, registry, or git URL), and the bundled license text (`null` if the crate ships none). |

Rules enforced by `BundleManifest::validate`:

- Paths are slash-separated, at most 240 bytes and 16 levels, ASCII `[A-Za-z0-9._+@ -]`, with no empty, `.` or `..` segments, no Windows device names (`CON`, `NUL`, `COM1`, ...), and no segment starting or ending with a space or ending with a dot. Paths are unique case-insensitively, and no path is both a file and a directory.
- `bin/aidex-launcher`, `bin/aidex`, `bin/aidexd`, and `bin/aidex-sandbox-exec` (with `.exe` on Windows) must each be listed exactly once with roles `launcher`, `cli`, `daemon`, and `helper`, and be executable.
- Roles and locations: `launcher`, `cli`, `daemon`, `helper` under `bin/`; `runtime` under `bin/` or `runtime/`; `desktop` under `desktop/`; `installer` under `installer/`; `doc` under `licenses/` or `docs/`. Critical roles (verified on every launch): `launcher`, `cli`, `daemon`, `helper`, `runtime`.
- Digests are 64 lowercase hex digits; a license `path` must be a listed `doc` file.

`BundleManifest::to_json` writes pretty JSON with a trailing newline in field order; with `normalize()` applied, equal manifests are byte-identical.

## `app/current.json` (schema 1)

```json
{
  "schema_version": 1,
  "targets": {
    "windows-x64": {
      "version": "0.1.0",
      "dir": "versions/0.1.0-windows-x64",
      "manifest_sha256": "<sha256 of that directory's bundle.json>",
      "previous": { "version": "0.0.9", "dir": "versions/0.0.9-windows-x64", "manifest_sha256": "<64 hex>" }
    }
  }
}
```

- One entry per target, because a drive carries bundles for every host it is used on. Keys must look like target names (`[a-z0-9]+(-[a-z0-9]+)*`); unknown targets are kept when the file is rewritten.
- `dir` is always `versions/<one segment>` (no traversal, no separators, not hidden); `previous` is `null` or the entry that was active before (the updater's rollback candidate). Fields are closed.
- `manifest_sha256` pins the exact `bundle.json`: a swapped or edited manifest is refused even if it matches the files.
- Only `install` / `CurrentPointer::store` write it, by temporary file, fsync, and rename (directory fsync is best effort, and a no-op on Windows).

## Launcher behaviour

1. Locate itself (`current_exe`, canonicalized) and take the nearest ancestor (up to six levels) that contains `aidex-drive.json` as the drive. `AIDEX_DRIVE_ROOT` from the caller is ignored. No autorun, no install, no registry, no host configuration.
2. Open the drive with `DrivePaths::open`, which parses and validates the closed-schema drive manifest and requires schema v2 (a v1 drive is told to run `aidex init`). The launcher must be at the drive root or in `app/launcher/<host target>/`; a copy anywhere else refuses to run.
3. Read `app/current.json` (a regular file, at most 1 MiB) and the entry for the host's target.
4. Check that `app/` and `app/versions/` are real directories and the entry's `dir` stays inside `app/versions/`; read the bundle's `bundle.json` (a regular file, at most 4 MiB) and compare its SHA-256 with `manifest_sha256` **before** parsing it.
5. Validate the manifest; require `target` = host target, `version` = pointer version, `min_drive_schema` ≤ the drive schema, and `host_extraction` = false.
6. Verify the critical files — `bin/` launcher, CLI, daemon, helper, and runtime files: every path component is a real directory or regular file (no symbolic links or junctions), and size and SHA-256 match. `bin/` must contain nothing that is not listed, so a planted library next to the binaries is refused.
7. Run `bin/aidex` with the caller's arguments unchanged (no arguments: `aidex`'s default), the caller's working directory and standard streams, and:
   - `AIDEX_DRIVE_ROOT` = the drive root (without the Windows `\\?\` prefix),
   - `AIDEX_BUNDLE_DIR`, `AIDEX_BUNDLE_VERSION`, `AIDEX_BUNDLE_TARGET`, `AIDEX_LAUNCHER_VERSION`,
   - `PATH` = the bundle's `bin/` followed by the inherited `PATH`.

   On Linux and macOS the launcher `exec`s the CLI (same process); on Windows it waits and exits with the CLI's exit code, ignoring Ctrl+C itself so the CLI handles it.

Reserved options (first argument only; anything else, including other `--launcher-*` spellings being rejected, goes to `aidex`): `--launcher-info` verifies as above and prints a one-line JSON report (launcher version, marker, drive root, target, bundle version/dir/path/digest/commit, scope, number of verified files, previous) without running the CLI; `--launcher-verify` does the same while verifying **every** listed file and refusing any unlisted file in the bundle.

Exit codes (on stderr: `AiDEX launcher: <message>` and a `hint:` line):

| Code | Meaning |
| --- | --- |
| CLI's code | The CLI ran; its exit status is passed through. |
| 1 | Unexpected I/O error or the CLI could not be started. |
| 2 | Unknown `--launcher-*` option. |
| 4 | Not on an initialized AiDEX drive, misplaced launcher, or an invalid / v1 drive manifest. |
| 8 | No bundle installed (`app/current.json` missing) or none for this host's target. |
| 10 | Incompatible: wrong-target bundle, newer pointer or manifest schema, newer drive schema, host extraction requested, or an unnamed host platform. |
| 11 | Integrity failure: hash or size mismatch, missing file, link, unlisted file in `bin/`, pointer escaping `app/versions/`, manifest not matching its pin, version disagreement, or malformed pointer/manifest. Nothing was run. |
| 126 | The CLI is not executable (for example a `noexec` or `showexec` mount). |

The launcher writes nothing — not to the host and not to the drive — and extracts nothing (bundles declare `host_extraction: false`), so there is no host residue for the cleanup manager (#28) to track. The Unix `aidex` script maps `uname -s`/`uname -m` to a target and `exec`s `app/launcher/<target>/aidex-launcher`, exiting 126/127 with a message when that launcher is missing or not executable.

## Licenses

The inventory is the normal-dependency closure of the four shipped packages for the target platform, from `cargo metadata --locked --filter-platform <triple>`; build and dev dependencies are not linked and are excluded, proc-macro crates are included. Each crate's declared SPDX expression is recorded, and its `LICENSE*`, `LICENCE*`, `COPYING*`, `NOTICE*`, `COPYRIGHT*`, and `UNLICENSE*` files (plus `license-file`) are concatenated into `licenses/third-party/<name>-<version>.txt`. Workspace crates are covered by the repository's MIT `LICENSE`, shipped as `licenses/aidex/LICENSE`. Crates with no license file in their package are listed with `path: null` and reported by `build`. This is license metadata, not an SBOM: there is no CycloneDX/SPDX document, no source hashes, and no provenance attestation — issue #27 adds those.

## Verification and its limits

Verification detects corruption, partial copies, accidental edits, wrong-platform bundles, and files planted next to the binaries, and it fails closed. It is **not** a defence against someone who can write to the drive: they can replace `AiDEX.exe`, `aidex`, or the pointer itself. Signatures with a trust anchor outside the drive are issue #27. Other known gaps:

- Files are hashed and then executed by path, so a process that rewrites the drive between the check and `exec` is not detected.
- Windows resolves a launcher's DLL imports before `main` runs, so a DLL planted next to `AiDEX.exe` at the drive root could be loaded before any check (the bundle's own `bin/` is checked).
- Binaries are unsigned. Windows SmartScreen and Mark-of-the-Web may warn for archives downloaded from the internet; macOS Gatekeeper quarantines downloaded archives (`xattr -dr com.apple.quarantine <dir>` after verifying `SHA256SUMS`).

## Reproducibility

Deterministic by construction: build inputs (`--locked`, pinned toolchain channel, fixed flags), file order, the manifest bytes, `created_at` (commit time), and archive metadata (sorted entries, entry times = `created_at`, modes 0755/0644, owner 0:0, empty user/group names, gzip header without name or time, fixed compression level). Rebuilding one commit on one machine with the same toolchain reproduces the archive in the tests and local runs; bit-for-bit identical binaries across machines or toolchain patch releases are **not** verified, and a dirty tree (`source_dirty: true`) is not reproducible by definition. `SHA256SUMS` describes what was built, not that anyone else rebuilt it.

## Smoke test

```sh
cargo run -p aidex-packager -- --json smoke --archive dist/aidex-0.1.0-windows-x64.zip --work <empty dir>
```

`smoke` checks the archive against `SHA256SUMS` (when present next to it), initializes `<work>/drive` as a developer drive, installs the archive, and runs the drive-root entry (`AiDEX.exe` or `./aidex`) with `--launcher-verify`, `--version`, `doctor --json`, `daemon start`, `daemon status --json`, and `daemon stop`. It requires success for each, except that `doctor` may exit 1 (it requires Oh My Pi and a removable drive, which CI folders are not) as long as it prints a well-formed report from the bundled CLI version. It then fails if new entries appeared in the home directory, the per-user config/data/state/cache roots (`%APPDATA%` and `%LOCALAPPDATA%` on Windows, XDG directories on Linux, `~/Library/{Application Support,Caches,Preferences,Logs}` on macOS), or AiDEX-named entries in the system temp directory, or if the drive root holds anything but the manifest, `.aidex`, launcher entries, and layout directories. The host check lists those directories' top level only; deeper writes inside existing directories (for example PowerShell's module cache when `aidexd` is started through `Start-Process`) are not detected.

## Platform support

| Target | Built by CI (`package.yml`) | Smoke-tested | Status |
| --- | --- | --- | --- |
| `windows-x64` | yes | CI (`windows-latest`) and locally on Windows 11 | Supported for the CLI bundle. |
| `linux-x64` (musl) | yes | CI (`ubuntu-latest`) | Supported for the CLI bundle; on FAT32 drives mounted with `showexec` (the udisks default) nothing without `.exe` is executable, so remount with exec or use exFAT/ext4. |
| `macos-arm64` | yes | CI (`macos-latest`) | CI-tested only; no `AiDEX.app` (use `./aidex` from Terminal). |
| `linux-arm64` | no | no | Not supported: the target name exists, but no bundle is built or tested. |
| `macos-x64` | no | no | Not supported: the target name exists, but no bundle is built or tested. |

A macOS `AiDEX.app` entry (a signed and notarized app bundle that opens Terminal on `./aidex`) is future work; it needs signing infrastructure that does not exist yet.

## Not included yet

- Signatures, SBOM, provenance, update channels, and rollback commands (#27 — it builds on `CurrentPointer::switch`, `previous`, and `install_with`).
- Pruning old `app/versions/` directories.
- Bun and Oh My Pi: `aidex chat` and `aidex run` still need Oh My Pi installed on the host; the native agent and local model runtimes (#15) are not bundled.
- Building the Tauri desktop and installer apps.
