// Mirrors `aidex_sandbox::BackendCapabilities` (crates/sandbox/src/capabilities.rs, `version` 1) and
// `SandboxLevel` (crates/sandbox/src/level.rs). Hand-written until schema generation exists.

export type SandboxLevel = "direct" | "brokered" | "restricted" | "ephemeral";

/** Levels lowest first, with the specification's numeric rank. */
export const sandboxLevels: readonly { level: SandboxLevel; rank: number }[] = [
  { level: "direct", rank: 0 },
  { level: "brokered", rank: 1 },
  { level: "restricted", rank: 2 },
  { level: "ephemeral", rank: 3 },
];

export type Requirement =
  | "process_tree_termination"
  | "process_count_limit"
  | "memory_limit"
  | "cpu_time_limit"
  | "wall_timeout"
  | "env_isolation"
  | "filesystem_read_confinement"
  | "filesystem_write_confinement"
  | "network_deny"
  | "network_allowlist"
  | "ephemeral_environment";

export type Enforcement =
  | { state: "enforced"; mechanism: string }
  | { state: "watchdog"; mechanism: string; interval_ms: number }
  | { state: "not_enforced"; reason: string }
  | { state: "unavailable"; reason: string };

export type Capability = {
  requirement: Requirement;
  enforcement: Enforcement;
  /** Omitted when there are none. */
  limitations?: string[];
};

export type BackendKind = "windows_job_object" | "linux_landlock" | "macos_seatbelt" | "unsupported";

export type HelperStatus = { state: "available"; path: string } | { state: "missing"; path: string; reason: string };

export type BackendCapabilities = {
  version: number;
  backend: BackendKind;
  os: string;
  helper: HelperStatus;
  /** One entry per requirement, in report order. */
  capabilities: Capability[];
  /** Highest level whose baseline is enforceable; `null` when nothing can launch (e.g. missing helper). */
  max_level: SandboxLevel | null;
  /** Omitted when empty. */
  facts?: string[];
};

/** Whether the enforcement applies anything (`enforced` or `watchdog`). */
export const isActive = (enforcement: Enforcement): boolean =>
  enforcement.state === "enforced" || enforcement.state === "watchdog";

/** Rank of a level (0–3). */
export const rankOf = (level: SandboxLevel): number => sandboxLevels.find((entry) => entry.level === level)?.rank ?? 0;
