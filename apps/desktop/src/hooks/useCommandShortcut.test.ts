import { fireEvent, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { isApplePlatform } from "../lib/platform";
import { useCommandShortcut } from "./useCommandShortcut";

vi.mock("../lib/platform", () => ({ isApplePlatform: vi.fn() }));

const press = (init: KeyboardEventInit) => fireEvent.keyDown(window, init);

describe("useCommandShortcut", () => {
  it("runs on Ctrl+key outside Apple platforms and prevents the browser default", () => {
    vi.mocked(isApplePlatform).mockReturnValue(false);
    const handler = vi.fn();
    renderHook(() => useCommandShortcut("k", handler));

    const handled = !press({ key: "K", ctrlKey: true });

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handled).toBe(true);
  });

  it("runs on ⌘+key on Apple platforms and ignores Ctrl there", () => {
    vi.mocked(isApplePlatform).mockReturnValue(true);
    const handler = vi.fn();
    renderHook(() => useCommandShortcut("k", handler));

    press({ key: "k", ctrlKey: true });
    press({ key: "k", metaKey: true });

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it.each([
    ["without the modifier", { key: "k" }],
    ["with Shift", { key: "k", ctrlKey: true, shiftKey: true }],
    ["with Alt", { key: "k", ctrlKey: true, altKey: true }],
    ["with both Ctrl and ⌘", { key: "k", ctrlKey: true, metaKey: true }],
    ["for another key", { key: "j", ctrlKey: true }],
    ["for a held-down repeat", { key: "k", ctrlKey: true, repeat: true }],
  ])("ignores the key %s", (_case, init) => {
    vi.mocked(isApplePlatform).mockReturnValue(false);
    const handler = vi.fn();
    renderHook(() => useCommandShortcut("k", handler));

    press(init);

    expect(handler).not.toHaveBeenCalled();
  });

  it("stops listening once unmounted", () => {
    vi.mocked(isApplePlatform).mockReturnValue(false);
    const handler = vi.fn();
    const { unmount } = renderHook(() => useCommandShortcut("k", handler));

    unmount();
    press({ key: "k", ctrlKey: true });

    expect(handler).not.toHaveBeenCalled();
  });
});
