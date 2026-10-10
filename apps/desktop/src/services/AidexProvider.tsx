import { createContext, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { aidexStatus, selectDrive, startDaemon, stopDaemon } from "@aidex/client/desktop";
import { ErrorKind, toAidexError, type AidexError } from "@aidex/client/errors";
import type { AidexStatus } from "@aidex/client/dto/status";

/** How often the session status is refreshed while the app is open. */
const POLL_MS = 10_000;

export type AidexAction = "start" | "stop" | "select";

export type AidexConnection = {
  /** Last `aidex_status` result; `null` until the first one arrives. */
  status: AidexStatus | null;
  /** Why `aidex_status` itself failed (for example `bridge_unavailable` outside the app). */
  error: AidexError | null;
  loading: boolean;
  /** The lifecycle action in flight, if any. */
  pending: AidexAction | null;
  /** Why the last start, stop, or drive selection failed. */
  actionError: AidexError | null;
  refresh: () => Promise<void>;
  /** Resolve `true` when the action succeeded. */
  startDaemon: () => Promise<boolean>;
  stopDaemon: () => Promise<boolean>;
  selectDrive: (path: string) => Promise<boolean>;
  /**
   * Identity of the drive and daemon the screens read from. Data hooks include it in their
   * keys, so they refetch when the drive changes or the daemon starts, stops, or restarts.
   */
  connectionKey: string;
};

export const AidexContext = createContext<AidexConnection | null>(null);

function connectionKeyOf(status: AidexStatus | null): string {
  if (!status) return "pending";
  const drive = status.drive.state === "resolved" ? status.drive.root : status.drive.state;
  const daemon = status.daemon.state === "running" ? `running:${status.daemon.status.pid}` : status.daemon.state;
  return `${drive}|${daemon}`;
}

/** Holds the session's drive and daemon status for every screen, including the launch flow. */
export function AidexProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AidexStatus | null>(null);
  const [error, setError] = useState<AidexError | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState<AidexAction | null>(null);
  const [actionError, setActionError] = useState<AidexError | null>(null);
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const next = await aidexStatus();
      if (!mounted.current) return;
      setStatus(next);
      setError(null);
    } catch (caught) {
      if (mounted.current) setError(toAidexError(caught));
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, []);

  const run = useCallback(async (action: AidexAction, job: () => Promise<AidexStatus>) => {
    setPending(action);
    setActionError(null);
    try {
      const next = await job();
      if (mounted.current) setStatus(next);
      return true;
    } catch (caught) {
      if (mounted.current) setActionError(toAidexError(caught));
      return false;
    } finally {
      if (mounted.current) setPending(null);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => {
      mounted.current = false;
    };
  }, [refresh]);

  // Outside the app there is nothing to poll.
  const bridgeMissing = error?.kind === ErrorKind.bridgeUnavailable;
  useEffect(() => {
    if (bridgeMissing) return;
    const timer = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(timer);
  }, [bridgeMissing, refresh]);

  const value = useMemo<AidexConnection>(
    () => ({
      status,
      error,
      loading,
      pending,
      actionError,
      refresh,
      startDaemon: () => run("start", startDaemon),
      stopDaemon: () => run("stop", stopDaemon),
      selectDrive: (path: string) => run("select", () => selectDrive(path)),
      connectionKey: connectionKeyOf(status),
    }),
    [status, error, loading, pending, actionError, refresh, run],
  );

  return <AidexContext.Provider value={value}>{children}</AidexContext.Provider>;
}
