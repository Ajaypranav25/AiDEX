# Launch feature

Owns the screens shown before the workspace. `../../app/App.tsx` steps through them in order:

1. `SelectDrivePage` — only when no drive resolved (AiDEX was not started from its drive and `AIDEX_DRIVE_ROOT` is unset or unusable). The chosen folder goes to `select_drive` and is kept for this session only; "Continue without a drive" enters the sample-data preview.
2. `UnlockPage` — the live drive (volume label, root, kind, size, where it came from), AiDEX Core status with Start, and the drive's vault from `vault.status`: `missing` shows `VaultSetupForm` (`vault.init`, passphrase of at least 8 characters, optional recovery key shown exactly once), `locked` shows `UnlockForm` (`vault.unlock` with the passphrase or recovery key; a wrong one is `vault_locked`), `unlocked` shows the secret count, and `unavailable` explains why nothing changed. Secrets go straight to the daemon and are cleared from state on submit. "Continue without unlocking" is always available.
3. `RuntimeDetectionPage` — live `host_profile` (OS, architecture, logical CPUs, RAM where reported, Git/Rust, shells, local model runtimes on PATH), `sandbox_capabilities` (maximum level, protections applied), and `drive_status` (schema, free space, kind, migration advice). Progress counts settled checks; unknown values read "Unknown".
