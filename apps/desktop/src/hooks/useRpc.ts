import { call, type RpcMethod, type RpcParams, type RpcResult } from "@aidex/client/rpc";
import { useAidexStatus } from "./useAidexStatus";
import { useBridgeQuery, type BridgeQuery } from "./useBridgeQuery";

/**
 * Call a typed daemon method and keep its result. It runs only while the daemon answers, and
 * refetches when the drive or daemon changes, when `params` change (compared as JSON), or on `refetch`.
 */
export function useRpc<M extends RpcMethod>(
  method: M,
  params?: RpcParams<M>,
  options: { enabled?: boolean } = {},
): BridgeQuery<RpcResult<M>> {
  const { connectionKey, daemonRunning } = useAidexStatus();
  const key = `${connectionKey}|${method}|${JSON.stringify(params ?? null)}`;
  return useBridgeQuery(() => call(method, params), key, daemonRunning && (options.enabled ?? true));
}
