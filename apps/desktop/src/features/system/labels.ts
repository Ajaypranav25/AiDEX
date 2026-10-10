// Plain-language labels for live Core data. Unknown values say "Unknown"; nothing here invents a value.
import type { DriveSource } from "@aidex/client/dto/drive";
import type { Probe } from "@aidex/client/dto/host";
import type { Enforcement, Requirement, SandboxLevel } from "@aidex/client/dto/sandbox";
import { rankOf } from "@aidex/client/dto/sandbox";

const OS_LABELS: Record<string, string> = { windows: "Windows", linux: "Linux", macos: "macOS" };

export const osLabel = (os: string): string => OS_LABELS[os] ?? os;

const LEVEL_NAMES: Record<SandboxLevel, string> = {
  direct: "Direct",
  brokered: "Brokered",
  restricted: "Restricted",
  ephemeral: "Ephemeral",
};

/** "Brokered (level 1)". */
export const levelLabel = (level: SandboxLevel): string => `${LEVEL_NAMES[level]} (level ${rankOf(level)})`;

export const requirementLabels: Record<Requirement, string> = {
  process_tree_termination: "Process tree termination",
  process_count_limit: "Process count limit",
  memory_limit: "Memory limit",
  cpu_time_limit: "CPU time limit",
  wall_timeout: "Wall-clock timeout",
  env_isolation: "Environment isolation",
  filesystem_read_confinement: "Filesystem read confinement",
  filesystem_write_confinement: "Filesystem write confinement",
  network_deny: "Network deny",
  network_allowlist: "Network allowlist",
  ephemeral_environment: "Ephemeral environment",
};

const ENFORCEMENT_LABELS: Record<Enforcement["state"], string> = {
  enforced: "Enforced",
  watchdog: "Watchdog",
  not_enforced: "Not enforced",
  unavailable: "Unavailable here",
};

export const enforcementLabel = (enforcement: Enforcement): string => ENFORCEMENT_LABELS[enforcement.state];

export const enforcementTone = (enforcement: Enforcement): "success" | "neutral" | "warning" =>
  enforcement.state === "enforced" ? "success" : enforcement.state === "watchdog" ? "neutral" : "warning";

/** The mechanism, or why nothing is applied. */
export const enforcementDetail = (enforcement: Enforcement): string =>
  enforcement.state === "enforced"
    ? enforcement.mechanism
    : enforcement.state === "watchdog"
      ? `${enforcement.mechanism} (sampled every ${enforcement.interval_ms} ms)`
      : enforcement.reason;

export const driveSourceLabels: Record<DriveSource, string> = {
  selected: "Chosen for this session",
  environment: "From AIDEX_DRIVE_ROOT",
  executable: "The drive AiDEX runs from",
};

export const probeLabel = <T,>(probe: Probe<T>): string =>
  probe.state === "available" ? "Found" : probe.state === "unavailable" ? "Not found" : "Unknown";
