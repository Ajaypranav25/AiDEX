import { useCallback, useEffect, useRef, useState } from "react";
import { call } from "@aidex/client/rpc";
import { toAidexError } from "@aidex/client/errors";

type FlowStart = { flow_id: string; authorization_url: string; provider_id: string };
type FlowStatus = {
  state: string;
  provider_id: string;
  account_label?: string | null;
  error?: string | null;
};

export type AuthFlowState =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "waiting"; flowId: string; providerId: string }
  | { phase: "connected"; providerId: string; label: string | null }
  | { phase: "failed"; message: string }
  | { phase: "cancelled" };

export type AuthFlowAdapter = {
  status: (flowId: string) => Promise<FlowStatus>;
  cancel: (flowId: string) => Promise<unknown>;
};

/** `auth.*` for provider accounts (ChatGPT sign-in). */
export const accountAuthAdapter: AuthFlowAdapter = {
  status: (flowId) => call("auth.status", { flow_id: flowId }),
  cancel: (flowId) => call("auth.cancel", { flow_id: flowId }),
};

/** `runtime.auth.*` for a Codex runtime that shares a ChatGPT sign-in. */
export function runtimeAuthAdapter(runtimeId: string): AuthFlowAdapter {
  return {
    status: (flowId) => call("runtime.auth.status", { id: runtimeId, flow_id: flowId }),
    cancel: (flowId) => call("runtime.auth.cancel", { id: runtimeId, flow_id: flowId }),
  };
}

/**
 * A browser sign-in: start it, open the authorization page in the system browser, then poll every
 * second until it connects, fails, or is cancelled. A pending flow is cancelled on unmount.
 */
export function useAuthFlow(adapter: AuthFlowAdapter, onConnected?: (providerId: string) => void) {
  const [state, setState] = useState<AuthFlowState>({ phase: "idle" });
  const adapterRef = useRef(adapter);
  adapterRef.current = adapter;
  const onConnectedRef = useRef(onConnected);
  onConnectedRef.current = onConnected;
  const flowId = state.phase === "waiting" ? state.flowId : null;
  const pendingRef = useRef<string | null>(null);
  pendingRef.current = flowId;

  useEffect(() => {
    if (!flowId) return;
    let live = true;
    const check = async () => {
      try {
        const status = await adapterRef.current.status(flowId);
        if (!live) return;
        if (status.state === "pending") return;
        // The flow is over: report it once, and unmounting from here on must not cancel it.
        live = false;
        pendingRef.current = null;
        if (status.state === "connected") {
          setState({ phase: "connected", providerId: status.provider_id, label: status.account_label ?? null });
          onConnectedRef.current?.(status.provider_id);
        } else if (status.state === "cancelled") {
          setState({ phase: "cancelled" });
        } else {
          setState({ phase: "failed", message: status.error ?? `Sign-in ${status.state}.` });
        }
      } catch (caught) {
        if (live) setState({ phase: "failed", message: toAidexError(caught).message });
      }
    };
    void check();
    const timer = setInterval(() => void check(), 1000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [flowId]);

  // Leaving the screen mid-sign-in cancels the flow rather than leaving it open.
  useEffect(
    () => () => {
      if (pendingRef.current) void adapterRef.current.cancel(pendingRef.current).catch(() => undefined);
    },
    [],
  );

  const start = useCallback(async (begin: () => Promise<FlowStart>) => {
    setState({ phase: "starting" });
    try {
      const flow = await begin();
      setState({ phase: "waiting", flowId: flow.flow_id, providerId: flow.provider_id });
      try {
        await call("auth.open", { authorization_url: flow.authorization_url });
      } catch (caught) {
        await adapterRef.current.cancel(flow.flow_id).catch(() => undefined);
        throw caught;
      }
    } catch (caught) {
      setState({ phase: "failed", message: toAidexError(caught).message });
    }
  }, []);

  const cancel = useCallback(async () => {
    const pending = pendingRef.current;
    setState({ phase: "cancelled" });
    if (pending) await adapterRef.current.cancel(pending).catch(() => undefined);
  }, []);

  const reset = useCallback(() => setState({ phase: "idle" }), []);

  return { state, start, cancel, reset };
}

/** One-line description of a flow for status text. */
export function authFlowText(state: AuthFlowState): string | null {
  switch (state.phase) {
    case "starting":
      return "Opening the sign-in page…";
    case "waiting":
      return "Finish signing in in your browser. This page updates when you are done.";
    case "connected":
      return state.label ? `Connected · ${state.label}` : "Connected";
    case "failed":
      return state.message;
    case "cancelled":
      return "Sign-in cancelled.";
    default:
      return null;
  }
}
