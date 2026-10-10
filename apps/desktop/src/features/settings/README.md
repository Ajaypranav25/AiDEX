# Settings feature

Preferences, diagnostics, and drive health for this AiDEX drive. Route: `settings`, tabs `appearance`, `shortcuts`, `updates`, `diagnostics`, and `drive` (order and labels in `tabs.ts`, shared with the sidebar).

- `SettingsPage` — header and underline tabs; each tab renders one panel.
- `AppearancePanel` — accent color (`SwatchPicker` over `accentPresets`, bound to `usePreferences().accent`, so it re-themes the app live), theme, density, reduce motion, and keyboard hints.
- `ShortcutsPanel` — shortcut table with the platform modifier (`useModifierKeyLabel`); only New session works today, the rest are marked planned.
- `UpdatesPanel` — version, release channel, check for updates, and how updates are applied (staged, verified, switched atomically, rolled back on failure).
- `DiagnosticsPanel` — doctor checks, system information, run diagnostics, and export logs.
- `DrivePanel` — drive identity and health, storage breakdown, integrity checks, host cleanup (`ResidueCleanup`), and safe eject.
- `SettingsSidebar` — one item per tab, active by the route's tab.

The accent color is the only setting with a real effect, and it lasts for this session only (preferences are held in memory). Switches, the channel, and cleanup selections are local state. Data comes from `sample.ts`; actions that need AiDEX Core (check for updates, run diagnostics, export logs, verify, clean up, safe eject) show a "Preview" toast instead of claiming they happened.
