import { callUntyped } from "@aidex/client/rpc";
import { useAidexStatus } from "./useAidexStatus";
import { useBridgeQuery } from "./useBridgeQuery";

/** A typed domain response while its Rust DTO is being added to the shared catalog. */
export function useDomain<T>(method: string, params?: unknown, enabled = true) {
  const { connectionKey, daemonRunning } = useAidexStatus();
  return useBridgeQuery(
    () => callUntyped(method, params) as Promise<T>,
    `${connectionKey}|${method}|${JSON.stringify(params ?? null)}`,
    daemonRunning && enabled,
  );
}
