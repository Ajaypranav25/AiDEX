// Typed wrappers for the installer's Tauri commands (apps/installer/src-tauri/src/commands.rs).
import { command } from "./bridge";
import type { VolumeInfo } from "./dto/drive";
import type { DriveInspection, InitReport } from "./dto/installer";

/** `list_drives`: mounted volumes with capacity, attachment, and AiDEX manifest state. */
export function listDrives(): Promise<VolumeInfo[]> {
  return command<VolumeInfo[]>("list_drives");
}

/** `inspect_drive`: whether `root` can be initialized and what AiDEX state it already holds. Read only. */
export function inspectDrive(root: string): Promise<DriveInspection> {
  return command<DriveInspection>("inspect_drive", { root });
}

/** `init_drive`: create the AiDEX layout and manifest on removable `root` (never formats or deletes). */
export function initDrive(root: string): Promise<InitReport> {
  return command<InitReport>("init_drive", { root });
}
