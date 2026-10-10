import type { ReactNode } from "react";
import type { AidexStatus } from "@aidex/client/dto/status";
import { AidexContext, type AidexConnection } from "../services/AidexProvider";

/** A resolved drive whose daemon answers. */
export const runningStatus: AidexStatus = {
  app_version: "0.1.0",
  drive: { state: "resolved", root: "H:\\", source: "executable" },
  daemon: {
    state: "running",
    started_by_app: true,
    status: {
      state: "ready",
      pid: 4242,
      contract_version: 1,
      version: "0.1.0",
      drive_root: "\\\\?\\H:\\",
      uptime_ms: 1000,
    },
  },
};

/** The `AidexProvider` value for tests, with a running daemon unless overridden. */
export function aidexConnection(overrides: Partial<AidexConnection> = {}): AidexConnection {
  return {
    status: runningStatus,
    error: null,
    loading: false,
    pending: null,
    actionError: null,
    refresh: async () => {},
    startDaemon: async () => true,
    stopDaemon: async () => true,
    selectDrive: async () => true,
    connectionKey: "H:\\|running:4242",
    ...overrides,
  };
}

/**
 * A `renderHook`/`render` wrapper serving `current()` as the AiDEX connection. Re-rendering picks up
 * a changed connection, so tests can switch drives or stop the daemon mid-test.
 */
export function aidexWrapper(current: () => AidexConnection) {
  return function AidexTestProvider({ children }: { children: ReactNode }) {
    return <AidexContext.Provider value={current()}>{children}</AidexContext.Provider>;
  };
}
