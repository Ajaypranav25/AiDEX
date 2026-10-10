// Mirrors `aidex_drive::{VolumeInfo, RemovableStatus}` (crates/drive/src/volume/mod.rs) and the drive
// health report built by the desktop (apps/desktop/src-tauri/src/commands/reports.rs) and the
// installer (apps/installer/src-tauri/src/reports.rs). Hand-written until schema generation exists.

/** How a volume is attached. `removable` and `usb_fixed` (portable SSDs) are portable media. */
export type RemovableStatus = "removable" | "usb_fixed" | "fixed" | "unknown";

export type VolumeInfo = {
  /** Volume root: `H:\` on Windows, the mount point elsewhere. */
  root: string;
  label: string | null;
  filesystem: string | null;
  total_bytes: number | null;
  free_bytes: number | null;
  removable: RemovableStatus;
  /** Holds the running operating system; never eligible for initialization. */
  system: boolean;
  has_aidex_manifest: boolean;
  /** Supported manifest schema, or `null` when absent, malformed, or unsupported. */
  schema_version: number | null;
};

/** `aidex_drive::DriveStatus` plus migration advice. */
export type DriveHealth = {
  writable: boolean;
  free_bytes: number | null;
  schema_version: number | null;
  current_schema_version: number;
  migration_required: boolean;
  migration_advice: string | null;
};

/** Where the desktop session's drive came from. */
export type DriveSource = "selected" | "environment" | "executable";

/** Portable media that is not the system volume: what `aidex_drive::init` accepts. */
export const isEligibleVolume = (volume: VolumeInfo): boolean =>
  (volume.removable === "removable" || volume.removable === "usb_fixed") && !volume.system;

export const removableLabels: Record<RemovableStatus, string> = {
  removable: "Removable drive",
  usb_fixed: "USB drive (fixed disk)",
  fixed: "Internal or fixed disk",
  unknown: "Unknown attachment",
};
