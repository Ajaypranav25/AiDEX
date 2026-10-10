import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorKind } from "@aidex/client/errors";
import { useBridgeQuery } from "./useBridgeQuery";

/** A promise the test settles by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((settle, fail) => {
    resolve = settle;
    reject = fail;
  });
  return { promise, resolve, reject };
}

describe("useBridgeQuery", () => {
  it("is loading until the command answers, then keeps its data", async () => {
    const answer = deferred<string>();
    const { result } = renderHook(() => useBridgeQuery(() => answer.promise, "status"));

    expect(result.current).toMatchObject({ loading: true, settled: false, data: undefined, error: null });

    await act(async () => answer.resolve("ready"));

    expect(result.current).toMatchObject({ loading: false, settled: true, data: "ready", error: null });
  });

  it("turns a rejection into an AidexError", async () => {
    const { result } = renderHook(() =>
      useBridgeQuery(() => Promise.reject({ code: "not_found", kind: ErrorKind.noDrive, message: "No drive." }), "drive"),
    );

    await waitFor(() => expect(result.current.settled).toBe(true));

    expect(result.current.data).toBeUndefined();
    expect(result.current.error).toMatchObject({ kind: ErrorKind.noDrive, message: "No drive." });
  });

  it("never lets a slow answer for an old key overwrite the current one", async () => {
    const answers = { first: deferred<string>(), second: deferred<string>() };
    const { result, rerender } = renderHook(({ key }) => useBridgeQuery(() => answers[key].promise, key), {
      initialProps: { key: "first" as keyof typeof answers },
    });

    rerender({ key: "second" });
    await act(async () => answers.second.resolve("second answer"));
    await act(async () => answers.first.resolve("first answer"));

    expect(result.current.data).toBe("second answer");
  });

  it("drops the previous key's data as soon as the key changes", async () => {
    const { result, rerender } = renderHook(({ key }) => useBridgeQuery(() => Promise.resolve(`data for ${key}`), key), {
      initialProps: { key: "a" },
    });
    await waitFor(() => expect(result.current.data).toBe("data for a"));

    rerender({ key: "b" });

    expect(result.current).toMatchObject({ data: undefined, loading: true });
    await waitFor(() => expect(result.current.data).toBe("data for b"));
  });

  it("refetches on demand and keeps showing the current data meanwhile", async () => {
    const load = vi.fn().mockResolvedValueOnce("before").mockReturnValueOnce(new Promise(() => {}));
    const { result } = renderHook(() => useBridgeQuery(load, "settings"));
    await waitFor(() => expect(result.current.data).toBe("before"));

    act(() => result.current.refetch());

    expect(load).toHaveBeenCalledTimes(2);
    expect(result.current).toMatchObject({ data: "before", loading: true, settled: false });
  });

  it("does nothing while disabled", () => {
    const load = vi.fn().mockResolvedValue("unused");

    const { result } = renderHook(() => useBridgeQuery(load, "drive|", false));

    expect(load).not.toHaveBeenCalled();
    expect(result.current).toMatchObject({ data: undefined, error: null, loading: false, settled: false });
  });
});
