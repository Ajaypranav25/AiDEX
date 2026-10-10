// Mirrors `AidexStatus`, `DriveState`, `DaemonState`, and `DriveReport` in
// apps/desktop/src-tauri/src/commands/reports.rs. Hand-written until schema generation exists.
import type { BridgeErrorPayload } from "../errors";
import type { DaemonStatus } from "./daemon";
import type { DriveHealth, DriveSource, VolumeInfo } from "./drive";

export type DriveState =
  | { state: "resolved"; root: string; source: DriveSource }
  | { state: "invalid"; source: DriveSource; path: string; error: BridgeErrorPayload }
  | { state: "none" };

export type DaemonState =
  | { state: "running"; status: DaemonStatus; started_by_app: boolean }
  | { state: "stopped"; error: BridgeErrorPayload }
  | { state: "incompatible"; error: BridgeErrorPayload }
  | { state: "unauthorized"; error: BridgeErrorPayload }
  | { state: "error"; error: BridgeErrorPayload }
  | { state: "no_drive" };

/** `aidex_status`: the session drive and whether its daemon answers. */
export type AidexStatus = {
  app_version: string;
  drive: DriveState;
  daemon: DaemonState;
};

/** `drive_status`: manifest health and the volume holding the drive. */
export type DriveReport = {
  root: string;
  source: DriveSource;
  health: DriveHealth | null;
  health_error: BridgeErrorPayload | null;
  volume: VolumeInfo | null;
  volume_error: BridgeErrorPayload | null;
};
