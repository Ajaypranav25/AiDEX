import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorKind } from "@aidex/client/errors";
import { subscribe, type SubscribeHandlers, type Subscription } from "@aidex/client/rpc";
import { aidexConnection, aidexWrapper } from "../test/aidex";
import { useSubscription } from "./useSubscription";

vi.mock("@aidex/client/rpc", () => ({ subscribe: vi.fn() }));

/** Subscriptions the hook opened, with the handlers it passed and a spy for `unsubscribe`. */
function daemonAcceptsSubscriptions() {
  const opened: { method: string; params: unknown; handlers: SubscribeHandlers; unsubscribe: ReturnType<typeof vi.fn> }[] = [];
  vi.mocked(subscribe).mockImplementation(async (method, params, handlers) => {
    const unsubscribe = vi.fn(async () => {});
    opened.push({ method, params, handlers, unsubscribe });
    return { id: `s${opened.length}`, initial: { cursor: opened.length }, unsubscribe } satisfies Subscription;
  });
  return opened;
}

describe("useSubscription", () => {
  it("subscribes while the daemon answers and forwards each notification", async () => {
    const opened = daemonAcceptsSubscriptions();
    const onData = vi.fn();
    const connection = aidexConnection();
    const { result } = renderHook(() => useSubscription("events.subscribe", { after: 3 }, onData), {
      wrapper: aidexWrapper(() => connection),
    });

    await waitFor(() => expect(result.current.active).toBe(true));
    act(() => opened[0].handlers.onData({ cursor: 4 }, "events.event"));

    expect(opened[0]).toMatchObject({ method: "events.subscribe", params: { after: 3 } });
    expect(result.current).toMatchObject({ initial: { cursor: 1 }, error: null, ended: null });
    expect(onData).toHaveBeenCalledWith({ cursor: 4 }, "events.event");
  });

  it("stays idle while the daemon is stopped", () => {
    daemonAcceptsSubscriptions();
    const connection = aidexConnection({ status: null, connectionKey: "pending" });

    const { result } = renderHook(() => useSubscription("events.subscribe", null, vi.fn()), {
      wrapper: aidexWrapper(() => connection),
    });

    expect(subscribe).not.toHaveBeenCalled();
    expect(result.current.active).toBe(false);
  });

  it("marks the stream inactive when the daemon ends it", async () => {
    const opened = daemonAcceptsSubscriptions();
    const connection = aidexConnection();
    const { result } = renderHook(() => useSubscription("events.subscribe", null, vi.fn()), {
      wrapper: aidexWrapper(() => connection),
    });
    await waitFor(() => expect(result.current.active).toBe(true));

    act(() => opened[0].handlers.onEnd?.({ subscription: "s1", reason: "connection_closed" }));

    expect(result.current).toMatchObject({ active: false, ended: { reason: "connection_closed" } });
  });

  it("reports why the daemon refused the subscription", async () => {
    vi.mocked(subscribe).mockRejectedValue({ code: "unavailable", kind: ErrorKind.subscriptionLimit, message: "Too many." });
    const connection = aidexConnection();

    const { result } = renderHook(() => useSubscription("events.subscribe", null, vi.fn()), {
      wrapper: aidexWrapper(() => connection),
    });

    await waitFor(() => expect(result.current.error?.kind).toBe(ErrorKind.subscriptionLimit));
    expect(result.current.active).toBe(false);
  });

  it("resubscribes when the daemon restarts and releases the old stream", async () => {
    const opened = daemonAcceptsSubscriptions();
    let connection = aidexConnection();
    const { result, rerender } = renderHook(() => useSubscription("events.subscribe", null, vi.fn()), {
      wrapper: aidexWrapper(() => connection),
    });
    await waitFor(() => expect(result.current.active).toBe(true));

    connection = aidexConnection({ connectionKey: "H:\\|running:5151" });
    rerender();

    await waitFor(() => expect(opened).toHaveLength(2));
    expect(opened[0].unsubscribe).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(result.current.initial).toEqual({ cursor: 2 }));
  });

  it("releases a subscription that is accepted only after unmounting", async () => {
    let accept!: (subscription: Subscription) => void;
    vi.mocked(subscribe).mockReturnValue(new Promise((resolve) => (accept = resolve)));
    const unsubscribe = vi.fn(async () => {});
    const connection = aidexConnection();
    const { unmount } = renderHook(() => useSubscription("events.subscribe", null, vi.fn()), {
      wrapper: aidexWrapper(() => connection),
    });

    unmount();
    await act(async () => accept({ id: "s1", initial: null, unsubscribe }));

    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });
});
