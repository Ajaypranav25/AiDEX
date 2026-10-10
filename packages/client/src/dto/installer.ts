// Mirrors the installer reports in apps/installer/src-tauri/src/reports.rs.
// Hand-written until schema generation exists.
import type { BridgeErrorPayload } from "../errors";
import type { DriveHealth, VolumeInfo } from "./drive";

/** `inspect_drive`: the volume, whether it can be initialized, and any existing AiDEX state. */
export type DriveInspection = {
  root: string;
  volume: VolumeInfo;
  /** Portable media that is not the system volume. */
  eligible: boolean;
  /** Why it cannot be initialized, when `eligible` is false. */
  ineligible_reason: string | null;
  /** Manifest health when the volume already holds a supported AiDEX manifest (read only). */
  health: DriveHealth | null;
  /** Why the existing manifest could not be read, when there is one. */
  health_error: BridgeErrorPayload | null;
};

/** `init_drive`: what initialization found and did. */
export type InitReport = {
  root: string;
  /** Manifest schema before init (`null` when the drive had none). */
  previous_schema_version: number | null;
  health: DriveHealth;
  /** Manifest-declared directories, relative to the root; all exist after init. */
  directories: string[];
};
