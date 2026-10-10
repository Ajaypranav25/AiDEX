// Sample data until AiDEX Core exposes updates, diagnostics (doctor), and drive health.
// Nothing here is read from, or written to, the drive or the host.

export type Shortcut = {
  id: string;
  action: string;
  /** Key pressed together with the platform's command modifier. */
  key: string;
  shift?: boolean;
  /** Whether the shortcut already works in this build. */
  wired: boolean;
};

export const sampleShortcuts: readonly Shortcut[] = [
  { id: "new-session", action: "New session", key: "K", wired: true },
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

export type DoctorCheck = {
  id: string;
  title: string;
  detail: string;
  tone: "success" | "warning";
  status: string;
};

export const sampleDoctorChecks: readonly DoctorCheck[] = [
  { id: "manifest", title: "Drive manifest valid", detail: "Schema 1 · signed by this drive", tone: "success", status: "Passed" },
  { id: "host", title: "Host capabilities detected", detail: "CPU, GPU, and memory reported", tone: "success", status: "Passed" },
  { id: "vault", title: "Vault reachable", detail: "Unlocked for this session", tone: "success", status: "Passed" },
  { id: "runtime", title: "Model runtime found", detail: "llama.cpp backend · Metal", tone: "success", status: "Passed" },
  { id: "sandbox", title: "Sandbox backend available", detail: "macOS sandbox profile loaded", tone: "success", status: "Passed" },
  {
    id: "gpu-driver",
    title: "GPU driver older than recommended",
    detail: "Inference still works; update the driver for full performance",
    tone: "warning",
    status: "Warning",
  },
];

export const sampleSystemInfo = [
  { label: "AiDEX", value: "0.1.0 (Stable)" },
  { label: "Operating system", value: "macOS 15.1 · arm64" },
  { label: "CPU", value: "Apple M2 Pro · 12 cores" },
  { label: "Memory", value: "32 GB unified" },
  { label: "Drive", value: "Samsung T7 · exFAT" },
] as const;

export const sampleDrive = {
  name: "Samsung T7",
  description: "Portable SSD · 1 TB · exFAT",
  capacityGb: 1000,
} as const;

export const sampleStorage = [
  { label: "Models", value: 312 },
  { label: "Workspaces", value: 148 },
  { label: "Sessions & history", value: 36 },
  { label: "Runtime & tools", value: 22 },
  { label: "Vault", value: 2 },
] as const;

export const sampleIntegrity = [
  { id: "manifest", title: "Manifest valid", detail: "Matches the signed drive layout" },
  { id: "checksums", title: "Checksums verified", detail: "1,284 files" },
  { id: "last-verified", title: "Last verified", detail: "Today, 8:47 AM" },
] as const;

export type ResidueItem = { id: string; label: string; description: string; sizeMb: number };

export const sampleResidue: readonly ResidueItem[] = [
  { id: "temp", label: "Temporary files", description: "Scratch files left on this computer by past sessions", sizeMb: 120 },
  { id: "model-cache", label: "Model cache", description: "Host-side copies made to speed up model loading", sizeMb: 340 },
  { id: "logs", label: "Logs older than 30 days", description: "Diagnostic logs no longer needed for support", sizeMb: 45 },
];
