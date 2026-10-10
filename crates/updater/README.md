# aidex-updater

Signed, staged, self-tested AiDEX updates with an atomic pointer switch, crash recovery, and rollback (issue #27). It builds on [`aidex-bundle`](../bundle/README.md), which owns every on-drive format (`bundle.json`, `bundle.sig`, `app/current.json`, `app/update-journal.json`) and the signature and trust-store code the launcher also runs; this crate owns the update *workflow*. User-facing instructions are in [`portable/packaging/README.md`](../../portable/packaging/README.md#updates).

## API

- `Updater::new(DrivePaths, Target, UpdaterConfig)` / `Updater::for_host(paths, config)`, `.with_events(Arc<dyn EventSink>)` (`update.*` events, best effort).
- `UpdaterConfig { trust: TrustStore, self_test: Arc<dyn SelfTest>, limits: UpdateLimits { max_archive_bytes (2 GiB), extract: ExtractLimits } }`; `UpdaterConfig::builtin()` (compiled-in trust + `ProcessSelfTest`), `new(trust)`, `with_self_test`, `with_limits`. The drive's `manifests/revoked-keys.json` is added to the trust store at every operation.
- `check(&UpdateSource, &ApplyOptions) -> CheckReport { source, package, checksum, candidate: CandidateInfo, current, previous, action }` — stages and verifies, changes nothing.
- `apply(&UpdateSource, &ApplyOptions) -> ApplyReport { update_id, action, checksum, candidate, from, to, self_test, launchers, pruned, warnings, restart_required }` and `apply_with(.., hook)`.
- `rollback(&RollbackOptions) -> RollbackReport { update_id, from, to, signature, launchers, warnings, restart_required }` and `rollback_with(.., hook)`.
- `recover() -> RecoveryReport { outcome: Clean | RolledBack | RolledForward | JournalSetAside, update_id, operation, interrupted_state, active, restored_pointer, removed, warnings }`.
- `status() -> UpdateStatus { target, current, previous, integrity, journal: Option<JournalSummary>, pending, versions, running_dir, restart_required }`.
- `integrity_report(&DrivePaths, Target, &TrustStore, VerifyScope, allow_unsigned) -> IntegrityReport { installed, verified, version, dir, manifest_sha256, verified_files, signature, signer_key_id, sbom: SbomStatus { present, valid, summary, error }, previous, pending_update, reason, error }` — exactly the launcher's check plus the SBOM (`aidex doctor`, `aidex update verify`).
- `UpdateSource::{LocalFile(path), Directory(path)}` (`from_path`); `ApplyOptions { allow_downgrade, allow_unsigned }`; `RollbackOptions { allow_unsigned }`; `UpdateAction::{Install, Upgrade, Repair, DowngradeToPrevious, UpToDate}`; `ChecksumStatus::{Verified, Absent, NotApplicable}`.
- `SelfTest` trait, `ProcessSelfTest { timeout }` (60 s per command), `SelfTestFn(closure)`; `Checkpoint` (12 apply / 6 rollback crash points), `Interrupted`, `Hook`.
- `sbom::{Sbom, Metadata, Component, ComponentType, Hash, LicenseChoice, Property, Dependency, SbomSummary}` (CycloneDX 1.5 model and `Sbom::validate`), `SBOM_BUNDLE_PATH = "docs/sbom.cdx.json"`; `sbom_status(bundle_dir, manifest)`.
- `UpdateError` with stable `reason()` (API `data.reason`), `code() -> ServiceErrorCode`, and `is_integrity()` (CLI exit 11): `busy`, `recovery_required`, `not_installed`, `no_previous`, `source_not_found`, `invalid_source`, `no_package`, `checksum_mismatch`, `checksum_missing`, `archive_too_large`, `unsafe_archive`, the signature reasons (`unsigned`, `malformed_signature`, `signature_manifest_mismatch`, `unknown_key`, `revoked_key`, `key_not_valid`, `bad_signature`), `tampered`, `missing_critical_file`, `wrong_target`, `downgrade`, `downgrade_not_previous`, `not_newer`, `invalid_version`, `incompatible_drive_schema`, `host_extraction`, `self_test_failed`, `interrupted`, `io`.

## State machine (spec §43)

```text
Idle ─▶ Staging ─▶ Verified ─▶ SelfTest ─▶ Switching ─▶ Committed
          │           │           │            │
          └───────────┴───────────┴────────────┴──▶ Aborted (refused, failed, or recovered)
```

| State (journal) | What happens | Crash recovery |
| --- | --- | --- |
| `staging` | `app/update.lock` taken; the package is checked (size, `SHA256SUMS` when present) and extracted with bounded, link-refusing extraction into `app/staging/<upd_id>/`. | roll back: remove staging |
| `verified` | `bundle.json` parsed; target, drive schema, host extraction checked; **signature** verified (trusted, unrevoked, in-window key) before any hashing; every file hashed, nothing unlisted; version rules applied; launcher destinations preflighted. | roll back |
| `self_test` | staged `aidex --version`, `aidexd --version`, `aidex doctor --json` with `AIDEX_DRIVE_ROOT`, a deadline, and bounded output. | roll back |
| `switching` | the version directory is chosen and recorded, the staged bundle is renamed into `app/versions/` (same volume), and `app/current.json` is replaced (temporary file, fsync, rename; the old entry becomes `previous`). | pointer selects the new bundle **and** it verifies → roll forward; otherwise roll back to `before` and remove the directory this update created |
| `committed` | drive-root launchers placed from the new bundle, `bundle.json`/`bundle.sig`/SBOM copied to `manifests/<version dir>/`, staging removed, then versions other than current and previous (`KEEP_VERSIONS = 2`) of this target pruned. These steps never undo the switch; failures are warnings. | nothing to undo; leftovers removed |

The journal `app/update-journal.json` (format in `aidex_bundle::UpdateJournal`) is written atomically *before* each step, records the whole pointer as it was (`before`), the target bundle (`to`), whether the operation created its version directory, and the signer. The rollback candidate is `previous` inside `app/current.json` (written atomically with the switch), so there is no separate `previous.json` that could disagree with the pointer after a crash.

`recover()` runs in the daemon right after it publishes its endpoint (so it owns the drive) and before it serves requests. It removes a leftover lock, temporary files of torn writes (`.current.json.*`, `.update-journal.json.*`), and `upd_*` staging debris, then applies the table above. A `current.json` the filesystem tore is rewritten from the journal's `before`; an unreadable journal is set aside as `update-journal.json.corrupt-<ms>`. In every case the pointer selects either the version that was active before or the new, fully verified one.

## Version rules

SemVer 2.0 precedence (build metadata ignored). Newer → `upgrade`. Same version and same `bundle.json` while the active bundle verifies → `up_to_date` (no-op); same version while the active bundle is damaged → `repair` (committed under a new directory name; `previous` kept); a different build of an intact active version → `not_newer`. Older → `downgrade`, unless `allow_downgrade` and the package is exactly the recorded previous bundle (version and `bundle.json` digest), which makes it a rollback delivered as a package. Non-SemVer versions are refused (`invalid_version`).

## Guarantees

- Nothing is switched until the package passed signature, integrity, version, and self-test checks; every refusal leaves `app/current.json` byte-for-byte unchanged, removes staging, and records `aborted` with the reason. Tests: `crates/updater/tests/refusals.rs`.
- Power loss at any of the 12 apply checkpoints or 6 rollback checkpoints, with torn temporary files, recovers to the old or the new valid version and leaves no lock, staging, temporary files, or stray version directories (`crates/updater/tests/recovery.rs`).
- Unsigned packages are refused unless `allow_unsigned` (development), which emits `update.verified { unsigned_allowed: true }` and a warning; an invalid or untrusted signature is never accepted.
- Running processes never swap their own binaries: a switch takes effect on the next launch (`restart_required`). Pruning never removes current, previous, any other target's entries, or the directory the running executable lives in.

## Limitations

- Offline/manual packages only (`LocalFile`, `Directory`); no HTTPS feed. A future feed must still verify the signature (never trust TLS alone).
- One update target per `Updater` (the host's in the daemon); bundles for other targets on the same drive are updated from those hosts. The self-test cannot run foreign binaries.
- The update lock is a `create_new` file, not an OS lock; the daemon is the only intended updater (`aidex-packager install` takes no lock).
- Revocation lists are additive and unsigned (removing an entry from a drive is possible for someone who can write the drive; compiled-in revocations cover that). Updates do not yet deliver revocation lists.
- `doctor`/status report the signer of the active bundle; there is no transparency log or provenance attestation (SLSA) yet.
