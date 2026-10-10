// Mirrors `aidex_host::HostProfile` (crates/host/src/lib.rs), profile `version` 1.
// Hand-written until schema-driven generation exists; keep field names identical to the Rust serde output.

/** A capability probe. `available` carries the evidence (a path, or `null` for facts without one). */
export type Probe<T> = { state: "available"; path: T } | { state: "unavailable" } | { state: "unknown" };

export type DirectoryFacts = {
  available: Probe<null>;
  /** Write permission is never tested by writing, so this is `unknown`. */
  writable: Probe<null>;
};

export type NamedTool = { name: string; result: Probe<string> };

export type HostProfile = {
  version: number;
  /** Compile-time OS identifier (`windows`, `linux`, `macos`). */
  os: string;
  /** Compile-time architecture identifier (`x86_64`, `aarch64`). */
  architecture: string;
  logical_cpu_count: number | null;
  /** Reported on Linux only; `null` elsewhere. */
  ram_bytes: number | null;
  executable: string | null;
  current_dir: string | null;
  temporary_directory: string | null;
  temporary_filesystem: DirectoryFacts;
  current_directory_filesystem: DirectoryFacts;
  shells: NamedTool[];
  git: Probe<string>;
  rust_toolchain: { rustc: Probe<string>; cargo: Probe<string> };
  inference_runtimes: NamedTool[];
  /** Not probed: always `unknown`. */
  network: Probe<null>;
  /** Not probed: always `unknown`. */
  gpu: Probe<null>;
  /** Not probed here; see `BackendCapabilities` from `sandbox_capabilities`. */
  sandbox: Probe<null>;
};

/** Whether a probe found its capability. */
export const isAvailable = <T>(probe: Probe<T>): probe is { state: "available"; path: T } => probe.state === "available";
