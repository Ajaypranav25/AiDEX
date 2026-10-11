// Display helpers shared by the apps. Unknown values render as "Unknown", never as an invented number.

export const UNKNOWN = "Unknown";

const BYTE_UNITS = ["bytes", "KB", "MB", "GB", "TB", "PB"] as const;

/** Binary-scaled size such as "114.6 GB" (1 GB = 1024³ bytes, as Windows reports). */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return UNKNOWN;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${value} ${value === 1 ? "byte" : "bytes"}`;
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${BYTE_UNITS[unit]}`;
}

/** Bytes as gigabytes (1024³), for planning estimates. */
export const bytesToGb = (bytes: number): number => bytes / 1024 ** 3;

/** A duration such as "2 h 5 min", "4 min 10 s", or "12 s". */
export function formatDuration(milliseconds: number | null | undefined): string {
  if (milliseconds === null || milliseconds === undefined || !Number.isFinite(milliseconds) || milliseconds < 0) return UNKNOWN;
  const seconds = Math.floor(milliseconds / 1000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours} h ${minutes} min`;
  if (minutes > 0) return `${minutes} min ${seconds % 60} s`;
  return `${seconds} s`;
}
