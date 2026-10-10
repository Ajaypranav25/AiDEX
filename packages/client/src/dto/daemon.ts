// Mirrors the `daemon.*` and `events.*` method DTOs in crates/aidexd/src/methods/ and
// `aidex_events::Event` (crates/events/src/lib.rs). Hand-written until schema generation exists.

/** Result of `daemon.status`. */
export type DaemonStatus = {
  state: "ready" | "stopping";
  pid: number;
  contract_version: number;
  version: string;
  /** As the daemon resolved it (on Windows with a `\\?\` prefix). */
  drive_root: string;
  uptime_ms: number;
};

/** Params of `events.replay`. */
export type ReplayParams = {
  workspace_id: string;
  /** Last acknowledged sequence; `0` replays from the start. */
  after: number;
  /** 1 to the daemon's batch maximum (its default). */
  limit?: number;
};

/**
 * One event kind. The catalog is internally tagged by `type` and is being expanded (events v2),
 * so screens must tolerate types they do not know.
 */
export type EventKind = { type: string } & Record<string, unknown>;

export type AidexEvent = {
  version: number;
  /** Monotonic per workspace: the replay cursor. */
  sequence: number;
  workspace_id: string;
  session_id: string | null;
  kind: EventKind;
};

/** Result of `events.replay`, ascending by sequence. */
export type ReplayBatch = { events: AidexEvent[] };
