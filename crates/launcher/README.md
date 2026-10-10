# aidex-launcher

The portable entry point. Installed at the drive root as `AiDEX.exe` (Windows) and under `app/launcher/<target>/aidex-launcher` (Linux, macOS; the root `./aidex` script selects it by `uname`). It finds the drive from its own location, verifies the drive manifest and the active bundle, and only then runs the bundle's `aidex` CLI with the caller's arguments. It needs no installation, autorun, administrator rights, Rust, Bun, or Node on the host.

The cargo binary is named `aidex-launcher` because `AiDEX.exe` would be the same file as the CLI's `aidex.exe` on case-insensitive filesystems; installs copy it to its root names.

## Behaviour

1. Canonicalize its own path; the nearest ancestor (six levels at most) holding `aidex-drive.json` is the drive. `AIDEX_DRIVE_ROOT` from the environment is ignored.
2. Open the drive through `aidex_drive::DrivePaths::open` (closed-schema manifest, schema v2) and require the launcher to be at the drive root or in `app/launcher/<host target>/`.
3. `aidex_bundle::resolve_active(paths, host, VerifyScope::Critical, policy)`: read `app/current.json`, keep `dir` inside `app/versions/`, compare `bundle.json` with its pinned SHA-256, require the host target, the pointer's version, a supported drive schema, and no host extraction, verify `bundle.sig` against the keys compiled into this launcher (`TrustStore::builtin`) minus the revocations in `manifests/revoked-keys.json`, then hash every critical file and refuse unlisted files in `bin/`. A bundle without `bundle.sig` is refused unless the host sets `AIDEX_ALLOW_UNSIGNED=1` (development only; a warning is printed on every launch); a present but invalid, unknown, revoked, or out-of-window signature is always refused.
   If `app/update-journal.json` names an unfinished update, the launcher prints a note and runs what the pointer selects (always a complete, verified old or new version, because the pointer is only replaced by an atomic rename); the daemon recovers the rest when it starts. The launcher never writes the journal.
4. Run `bin/aidex` with the arguments unchanged, setting `AIDEX_DRIVE_ROOT`, `AIDEX_BUNDLE_DIR`, `AIDEX_BUNDLE_VERSION`, `AIDEX_BUNDLE_TARGET`, `AIDEX_LAUNCHER_VERSION`, and `PATH` = bundle `bin/` + inherited `PATH`. Unix: `exec`. Windows: wait, pass the exit code through, and ignore Ctrl+C (the CLI receives it).

`--launcher-info` (critical scope) and `--launcher-verify` (every file) print a JSON report instead of running the CLI, including `bundle.signature` (`{status: verified, key_id, key_name, key_source}` or `{status: unsigned_allowed}`), `pending_update`, and `dev_trust`. Exit codes: 2 usage, 4 drive, 8 not installed, 10 incompatible, 11 integrity or signature failure (including an unreadable revocation list), 126 not executable, 1 other; otherwise the CLI's own code. The full contract, including the report fields, is in [`portable/packaging/README.md`](../../portable/packaging/README.md#launcher-behaviour).

The launcher writes nothing to the host or the drive, keeps no logs, and extracts nothing.

## Tests

`tests/launch.rs` installs a bundle whose `aidex` is `aidex-launcher-test-cli` (a test-only binary that reports its arguments and environment), signs it with a key generated at run time (trusted through `AIDEX_DEV_TRUSTED_KEYS`, which only test builds honour via the `dev-trust` feature), and runs the real launcher binary: passthrough arguments and exit codes, `AIDEX_DRIVE_ROOT`/`PATH`, the Unix entry script, no writes to the drive (also with an interrupted update journal), and refusal of tampered binaries, planted files, a missing pointer, pointers escaping `app/versions/`, wrong-target bundles, misplaced launchers, unsigned bundles (unless `AIDEX_ALLOW_UNSIGNED=1`), untrusted and revoked keys, and a tampered manifest re-pinned in the pointer.

## Limitations

- The trust anchor is compiled into the launcher on the drive: an attacker who can write the drive can replace the launcher itself (signatures protect against tampered bundles and updates, not against a replaced launcher).
- Windows loads a launcher's DLL imports before `main`, so a DLL planted next to `AiDEX.exe` at the drive root is outside the check.
- No macOS `AiDEX.app`; macOS users run `./aidex` from Terminal.
