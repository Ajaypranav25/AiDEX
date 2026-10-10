import { useEffect, useRef, useState } from "react";
import { toAidexError, type AidexError } from "@aidex/client/errors";
import { subscribe, type SubscriptionEnded } from "@aidex/client/rpc";
import { useAidexStatus } from "./useAidexStatus";

export type SubscriptionState = {
  /** The daemon accepted the stream and it has not ended. */
  active: boolean;
  /** What the subscribe handler returned (for example a cursor). */
  initial: unknown;
  error: AidexError | null;
  /** Set when the daemon ended the stream or its connection closed. */
  ended: SubscriptionEnded | null;
};

/**
 * Subscribe to a daemon stream while mounted and the daemon answers; `onData` receives each
 * notification in order. Resubscribes when the drive or daemon changes or `params` change
 * (compared as JSON), and unsubscribes on unmount.
 */
export function useSubscription(
  method: string,
  params: unknown,
  onData: (data: unknown, method: string) => void,
  options: { enabled?: boolean } = {},
): SubscriptionState {
  const { connectionKey, daemonRunning } = useAidexStatus();
  const [state, setState] = useState<SubscriptionState>({ active: false, initial: undefined, error: null, ended: null });
  const onDataRef = useRef(onData);
  onDataRef.current = onData;
  const paramsKey = JSON.stringify(params ?? null);
  const enabled = daemonRunning && (options.enabled ?? true);

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    let release: (() => Promise<void>) | null = null;
    setState({ active: false, initial: undefined, error: null, ended: null });
    subscribe(method, JSON.parse(paramsKey), {
      onData: (data, notificationMethod) => onDataRef.current(data, notificationMethod),
      onEnd: (ended) => !cancelled && setState((previous) => ({ ...previous, active: false, ended })),
    }).then(
      (subscription) => {
        if (cancelled) {
          void subscription.unsubscribe();
          return;
        }
        release = subscription.unsubscribe;
        setState((previous) => ({ ...previous, active: previous.ended === null, initial: subscription.initial }));
      },
      (error: unknown) => !cancelled && setState({ active: false, initial: undefined, error: toAidexError(error), ended: null }),
    );
    return () => {
      cancelled = true;
      void release?.();
    };
  }, [method, paramsKey, enabled, connectionKey]);

  return state;
}
