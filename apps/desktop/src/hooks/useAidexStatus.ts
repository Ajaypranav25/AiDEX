import { useContext } from "react";
import type { DaemonStatus } from "@aidex/client/dto/daemon";
import { AidexContext, type AidexConnection } from "../services/AidexProvider";

export type AidexStatusView = AidexConnection & {
  /** Display root of the session drive, when one is resolved. */
  driveRoot: string | null;
  /** `daemon.status` of the running daemon, when it answers. */
  daemon: DaemonStatus | null;
  daemonRunning: boolean;
  /** The page runs outside the AiDEX app (for example `next dev` in a browser). */
  bridgeMissing: boolean;
};

/** The session's drive and daemon status, with lifecycle actions. */
export function useAidexStatus(): AidexStatusView {
  const connection = useContext(AidexContext);
  if (!connection) throw new Error("useAidexStatus must be used inside AidexProvider");
  const { status, error } = connection;
  const daemon = status?.daemon.state === "running" ? status.daemon.status : null;
  return {
    ...connection,
    driveRoot: status?.drive.state === "resolved" ? status.drive.root : null,
    daemon,
    daemonRunning: daemon !== null,
    bridgeMissing: error?.kind === "bridge_unavailable",
  };
}
