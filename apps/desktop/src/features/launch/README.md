# Launch feature

Owns the screens shown before the workspace. `../../app/App.tsx` steps through them in order:

1. `SelectDrivePage` — only when no drive resolved (AiDEX was not started from its drive and `AIDEX_DRIVE_ROOT` is unset or unusable). The chosen folder goes to `select_drive` and is kept for this session only; "Continue without a drive" enters the sample-data preview.
2. `UnlockPage` — the live drive (volume label, root, kind, size, where it came from), AiDEX Core status with Start, and the vault state from `vault.status`: `method_not_found` means this build has no vault, so the page says "Vault not set up on this drive" and asks for no password. `UnlockForm` keeps the password and recovery-key form for the vault wave (issue #13); it is not rendered yet and never keeps a submitted secret.
3. `RuntimeDetectionPage` — live `host_profile` (OS, architecture, logical CPUs, RAM where reported, Git/Rust, shells, local model runtimes on PATH), `sandbox_capabilities` (maximum level, protections applied), and `drive_status` (schema, free space, kind, migration advice). Progress counts settled checks; unknown values read "Unknown".
