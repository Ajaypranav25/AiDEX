// Sample data for the settings that have no backend yet: shortcuts, updates (issue #27), and
// host residue cleanup (issue #28). Diagnostics and Drive are live. Nothing here is read from,
// or written to, the drive or the host.

export type Shortcut = {
  id: string;
  action: string;
  /** Key pressed together with the platform's command modifier. */
  key: string;
  shift?: boolean;
  /** Keys outside Apple platforms, for shortcuts that use another modifier there (Alt+← rather than Ctrl+[). */
  otherKeys?: readonly string[];
  /** Whether the shortcut already works in this build. */
  wired: boolean;
};

export const sampleShortcuts: readonly Shortcut[] = [
  { id: "new-session", action: "New session", key: "K", wired: true },
  { id: "go-back", action: "Go back", key: "[", otherKeys: ["Alt", "←"], wired: true },
  { id: "go-forward", action: "Go forward", key: "]", otherKeys: ["Alt", "→"], wired: true },
  { id: "search", action: "Search", key: "F", wired: false },
  { id: "toggle-sidebar", action: "Toggle sidebar", key: "B", wired: false },
  { id: "switch-mode", action: "Switch Chat/Agent", key: "M", shift: true, wired: false },
  { id: "open-settings", action: "Open settings", key: ",", wired: false },
];

export const updateChannels = [
  { value: "stable", label: "Stable" },
  { value: "beta", label: "Beta" },
] as const;

export type UpdateChannel = (typeof updateChannels)[number]["value"];

export const sampleVersion = { version: "0.1.0", lastChecked: "Today, 9:02 AM" } as const;

export type ResidueItem = { id: string; label: string; description: string; sizeMb: number };

export const sampleResidue: readonly ResidueItem[] = [
  { id: "temp", label: "Temporary files", description: "Scratch files left on this computer by past sessions", sizeMb: 120 },
  { id: "model-cache", label: "Model cache", description: "Host-side copies made to speed up model loading", sizeMb: 340 },
  { id: "logs", label: "Logs older than 30 days", description: "Diagnostic logs no longer needed for support", sizeMb: 45 },
];
