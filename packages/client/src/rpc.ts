import { command, listenTo } from "./bridge";
import type { DaemonStatus, ReplayBatch, ReplayParams } from "./dto/daemon";

/**
 * Daemon methods that exist today, with their params and results. Add an entry when a backend
 * registers a method (crates/aidexd/src/methods/); `callUntyped` reaches anything else.
 */
export type RpcMethods = {
  "daemon.status": { params: undefined; result: DaemonStatus };
  "daemon.ping": { params: undefined; result: { pong: true } };
  "events.replay": { params: ReplayParams; result: ReplayBatch };
};

export type RpcMethod = keyof RpcMethods;
export type RpcParams<M extends RpcMethod> = RpcMethods[M]["params"];
export type RpcResult<M extends RpcMethod> = RpcMethods[M]["result"];

/** Call a typed daemon method through the desktop bridge (`rpc_call`). */
export function call<M extends RpcMethod>(method: M, params?: RpcParams<M>): Promise<RpcResult<M>> {
  return command<RpcResult<M>>("rpc_call", { method, params: params ?? null });
}

/** Call any daemon method by name; errors keep the daemon's `kind` (e.g. `method_not_found`). */
export function callUntyped(method: string, params?: unknown): Promise<unknown> {
  return command<unknown>("rpc_call", { method, params: params ?? null });
}

/** Payload of the `aidex://notification` event. */
export type NotificationEvent = { subscription: string; method: string; data: unknown };
/** Payload of the `aidex://subscription-ended` event. */
export type SubscriptionEnded = { subscription: string; reason: string; error?: unknown };

export type SubscribeHandlers = {
  /** Each notification's `data`, in order. */
  onData: (data: unknown, method: string) => void;
  /** The stream ended on the daemon side or its connection closed (not called after `unsubscribe`). */
  onEnd?: (ended: SubscriptionEnded) => void;
};

export type Subscription = {
  id: string;
  /** What the daemon's subscribe handler returned (for example a cursor). */
  initial: unknown;
  unsubscribe: () => Promise<void>;
};

/** Notifications that can arrive before `rpc_subscribe` returns the id are buffered up to this many. */
const EARLY_BUFFER_LIMIT = 1024;

/**
 * Subscribe to a daemon stream (`rpc_subscribe`). Listeners attach before the subscribe call, so
 * notifications sent before it returns are replayed in order rather than lost.
 */
export async function subscribe(method: string, params: unknown, handlers: SubscribeHandlers): Promise<Subscription> {
  let id: string | null = null;
  let closed = false;
  const early: NotificationEvent[] = [];
  const earlyEnds: SubscriptionEnded[] = [];

  const stopNotifications = await listenTo<NotificationEvent>("aidex://notification", (event) => {
    if (closed) return;
    if (id === null) {
      if (early.length < EARLY_BUFFER_LIMIT) early.push(event);
    } else if (event.subscription === id) {
      handlers.onData(event.data, event.method);
    }
  });
  const stopEnds = await listenTo<SubscriptionEnded>("aidex://subscription-ended", (event) => {
    if (closed) return;
    if (id === null) earlyEnds.push(event);
    else if (event.subscription === id) finish(event);
  });

  const detach = () => {
    closed = true;
    stopNotifications();
    stopEnds();
  };
  const finish = (event: SubscriptionEnded) => {
    detach();
    handlers.onEnd?.(event);
  };

  let subscribed: { subscription: string; initial: unknown };
  try {
    subscribed = await command<{ subscription: string; initial: unknown }>("rpc_subscribe", {
      method,
      params: params ?? null,
    });
  } catch (error) {
    detach();
    throw error;
  }
  id = subscribed.subscription;
  for (const event of early.filter((candidate) => candidate.subscription === id)) handlers.onData(event.data, event.method);
  const ended = earlyEnds.find((candidate) => candidate.subscription === id);
  if (ended) finish(ended);

  return {
    id,
    initial: subscribed.initial,
    unsubscribe: async () => {
      if (closed) return;
      detach();
      await command<boolean>("rpc_unsubscribe", { subscription: subscribed.subscription });
    },
  };
}
