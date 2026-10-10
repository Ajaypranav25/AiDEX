import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorKind } from "./errors";
import { call, callUntyped, subscribe, type NotificationEvent, type SubscriptionEnded } from "./rpc";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const NOTIFICATION = "aidex://notification";
const ENDED = "aidex://subscription-ended";
const METHOD = "events.subscribe";

/** The Rust side's event bus: listeners registered through `listen`, by event name. */
const listeners = new Map<string, Set<EventCallback<unknown>>>();

function emit(event: string, payload: NotificationEvent | SubscriptionEnded) {
  for (const listener of listeners.get(event) ?? []) listener({ event, id: 0, payload });
}
const notify = (subscription: string, data: unknown) => emit(NOTIFICATION, { subscription, method: METHOD, data });
const end = (subscription: string) => emit(ENDED, { subscription, reason: "closed" });
const listenerCount = () => [...listeners.values()].reduce((total, set) => total + set.size, 0);

/** `rpc_subscribe` answers with `id` after running `beforeReturning`; `rpc_unsubscribe` reports success. */
function daemonSubscribes(id: string, beforeReturning: () => void = () => {}) {
  vi.mocked(invoke).mockImplementation(async (name) => {
    if (name === "rpc_subscribe") {
      beforeReturning();
      return { subscription: id, initial: { cursor: 7 } };
    }
    if (name === "rpc_unsubscribe") return true;
    throw new Error(`unexpected command ${name}`);
  });
}

beforeEach(() => {
  listeners.clear();
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    const set = listeners.get(event) ?? new Set();
    set.add(handler as EventCallback<unknown>);
    listeners.set(event, set);
    return () => void set.delete(handler as EventCallback<unknown>);
  });
});

describe("call", () => {
  it("sends the method and its params through rpc_call", async () => {
    vi.mocked(invoke).mockResolvedValue({ secrets: 2 });

    await expect(call("vault.unlock", { passphrase: "correct horse" })).resolves.toEqual({ secrets: 2 });
    expect(invoke).toHaveBeenCalledWith("rpc_call", { method: "vault.unlock", params: { passphrase: "correct horse" } });
  });

  it("sends null params for a method that takes none", async () => {
    vi.mocked(invoke).mockResolvedValue({});

    await call("daemon.status");
    await callUntyped("skills.list");

    expect(invoke).toHaveBeenNthCalledWith(1, "rpc_call", { method: "daemon.status", params: null });
    expect(invoke).toHaveBeenNthCalledWith(2, "rpc_call", { method: "skills.list", params: null });
  });
});

describe("subscribe", () => {
  it("replays, in order, the notifications that arrived before rpc_subscribe returned", async () => {
    daemonSubscribes("s1", () => {
      notify("s1", 1);
      notify("someone-else", "x");
      notify("s1", 2);
    });
    const onData = vi.fn();

    const subscription = await subscribe(METHOD, { from: 0 }, { onData });

    expect(invoke).toHaveBeenCalledWith("rpc_subscribe", { method: METHOD, params: { from: 0 } });
    expect(subscription).toMatchObject({ id: "s1", initial: { cursor: 7 } });
    expect(onData.mock.calls).toEqual([
      [1, METHOD],
      [2, METHOD],
    ]);
  });

  it("delivers later notifications for its own subscription only", async () => {
    daemonSubscribes("s1");
    const onData = vi.fn();
    await subscribe(METHOD, null, { onData });

    notify("s2", "not mine");
    notify("s1", "mine");

    expect(onData.mock.calls).toEqual([["mine", METHOD]]);
  });

  it("buffers a bounded number of early notifications", async () => {
    daemonSubscribes("s1", () => {
      for (let index = 0; index < 1100; index += 1) notify("s1", index);
    });
    const onData = vi.fn();

    await subscribe(METHOD, null, { onData });

    expect(onData).toHaveBeenCalledTimes(1024);
  });

  it("finishes at once when the stream ended before rpc_subscribe returned", async () => {
    daemonSubscribes("s1", () => end("s1"));
    const onData = vi.fn();
    const onEnd = vi.fn();

    await subscribe(METHOD, null, { onData, onEnd });
    notify("s1", "too late");

    expect(onEnd).toHaveBeenCalledWith({ subscription: "s1", reason: "closed" });
    expect(onData).not.toHaveBeenCalled();
    expect(listenerCount()).toBe(0);
  });

  it("reports the end of its own stream once and stops delivering", async () => {
    daemonSubscribes("s1");
    const onData = vi.fn();
    const onEnd = vi.fn();
    await subscribe(METHOD, null, { onData, onEnd });

    end("s2");
    end("s1");
    end("s1");
    notify("s1", "after the end");

    expect(onEnd).toHaveBeenCalledTimes(1);
    expect(onData).not.toHaveBeenCalled();
    expect(listenerCount()).toBe(0);
  });

  it("releases the daemon subscription once and never reports an end after unsubscribing", async () => {
    daemonSubscribes("s1");
    const onData = vi.fn();
    const onEnd = vi.fn();
    const subscription = await subscribe(METHOD, null, { onData, onEnd });

    await subscription.unsubscribe();
    await subscription.unsubscribe();
    notify("s1", "after unsubscribe");
    end("s1");

    expect(invoke).toHaveBeenCalledWith("rpc_unsubscribe", { subscription: "s1" });
    expect(vi.mocked(invoke).mock.calls.filter(([name]) => name === "rpc_unsubscribe")).toHaveLength(1);
    expect(onData).not.toHaveBeenCalled();
    expect(onEnd).not.toHaveBeenCalled();
    expect(listenerCount()).toBe(0);
  });

  it("detaches its listeners and rethrows when the daemon refuses the subscription", async () => {
    vi.mocked(invoke).mockRejectedValue({
      code: "unavailable",
      kind: ErrorKind.daemonUnavailable,
      message: "AiDEX Core is not running.",
    });

    await expect(subscribe(METHOD, null, { onData: vi.fn() })).rejects.toMatchObject({
      kind: ErrorKind.daemonUnavailable,
    });
    expect(listenerCount()).toBe(0);
  });
});
