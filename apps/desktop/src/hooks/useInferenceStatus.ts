import { useEffect, useRef } from "react";
import { useRpc } from "./useRpc";
import { useAidexStatus } from "./useAidexStatus";

/** Poll the inexpensive runtime status; refresh provider catalogs only on runtime changes. */
export function useInferenceStatus(onRuntimeChange?: () => void) {
  const status = useRpc("inference.status");
  const { connectionKey, daemonRunning } = useAidexStatus();
  const previous = useRef<{ connection: string; runtime: string } | null>(null);
  useEffect(() => {
    if (!daemonRunning || status.loading) return;
    const timer = setTimeout(
      status.refetch,
      status.data?.state === "starting" ? 1000 : 5000,
    );
    return () => clearTimeout(timer);
  }, [daemonRunning, status.loading, status.data, status.refetch]);
  useEffect(() => {
    if (!status.data) return;
    const runtime =
      status.data.state === "ready"
        ? `ready:${status.data.model_id}:${status.data.pid}`
        : "unavailable";
    if (
      (previous.current?.connection !== connectionKey ||
        previous.current.runtime !== runtime) &&
      (runtime !== "unavailable" ||
        previous.current?.runtime?.startsWith("ready:"))
    )
      onRuntimeChange?.();
    previous.current = { connection: connectionKey, runtime };
  }, [connectionKey, status.data, onRuntimeChange]);
  return status;
}
