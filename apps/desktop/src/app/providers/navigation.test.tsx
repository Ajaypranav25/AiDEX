import { act, fireEvent, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { isApplePlatform } from "../../lib/platform";
import type { Route } from "../../lib/routes";
import { HISTORY_LIMIT, NavigationProvider, useNavigation } from "./navigation";

vi.mock("../../lib/platform", () => ({ isApplePlatform: vi.fn(() => false) }));

function renderNavigation(initialRoute: Route = { page: "chat" }) {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <NavigationProvider initialRoute={initialRoute}>{children}</NavigationProvider>
  );
  return renderHook(() => useNavigation(), { wrapper });
}

describe("NavigationProvider history", () => {
  it("starts on the initial route with nowhere to go", () => {
    const { result } = renderNavigation({ page: "home" });

    expect(result.current).toMatchObject({ route: { page: "home" }, canGoBack: false, canGoForward: false });
  });

  it("goes back and forward through visited routes", () => {
    const { result } = renderNavigation();
    act(() => result.current.navigate({ page: "workspaces" }));
    act(() => result.current.navigate({ page: "workspace", id: "w1" }));

    act(() => result.current.back());
    expect(result.current).toMatchObject({ route: { page: "workspaces" }, canGoBack: true, canGoForward: true });

    act(() => result.current.back());
    expect(result.current).toMatchObject({ route: { page: "chat" }, canGoBack: false, canGoForward: true });

    act(() => result.current.forward());
    act(() => result.current.forward());
    expect(result.current).toMatchObject({ route: { page: "workspace", id: "w1" }, canGoForward: false });
  });

  it("stays put when there is nothing to go back or forward to", () => {
    const { result } = renderNavigation();

    act(() => result.current.back());
    act(() => result.current.forward());

    expect(result.current.route).toEqual({ page: "chat" });
  });

  it("drops the forward history when a new route opens", () => {
    const { result } = renderNavigation();
    act(() => result.current.navigate({ page: "history" }));
    act(() => result.current.back());

    act(() => result.current.navigate({ page: "settings" }));

    expect(result.current).toMatchObject({ route: { page: "settings" }, canGoForward: false });
    act(() => result.current.back());
    expect(result.current.route).toEqual({ page: "chat" });
  });

  it("adds no entry when the current route opens again", () => {
    const { result } = renderNavigation();
    act(() => result.current.navigate({ page: "models", tab: "local" }));

    act(() => result.current.navigate({ page: "models", tab: "local" }));
    act(() => result.current.back());

    expect(result.current.route).toEqual({ page: "chat" });
  });

  it("replaces the current entry without touching the rest of the history", () => {
    const { result } = renderNavigation();
    act(() => result.current.navigate({ page: "settings", tab: "appearance" }));
    act(() => result.current.navigate({ page: "security" }));
    act(() => result.current.back());

    act(() => result.current.replace({ page: "settings", tab: "drive" }));

    expect(result.current).toMatchObject({ route: { page: "settings", tab: "drive" }, canGoForward: true });
    act(() => result.current.forward());
    expect(result.current.route).toEqual({ page: "security" });
    act(() => result.current.back());
    act(() => result.current.back());
    expect(result.current.route).toEqual({ page: "chat" });
  });

  it(`keeps only the latest ${HISTORY_LIMIT} entries`, () => {
    const { result } = renderNavigation({ page: "workspace", id: "0" });
    for (let index = 1; index <= HISTORY_LIMIT + 10; index += 1) {
      act(() => result.current.navigate({ page: "workspace", id: String(index) }));
    }

    for (let step = 0; step < HISTORY_LIMIT + 10; step += 1) act(() => result.current.back());

    expect(result.current).toMatchObject({ route: { page: "workspace", id: "11" }, canGoBack: false });
  });
});

describe("NavigationProvider shortcuts", () => {
  /** A provider that has visited chat, then history, then went back to chat. */
  function renderMidHistory() {
    const rendered = renderNavigation();
    act(() => rendered.result.current.navigate({ page: "history" }));
    act(() => rendered.result.current.navigate({ page: "settings" }));
    act(() => rendered.result.current.back());
    return rendered;
  }

  it("goes back with Alt+← and forward with Alt+→ outside Apple platforms", () => {
    const { result } = renderMidHistory();

    const backDefault = fireEvent.keyDown(window, { key: "ArrowLeft", altKey: true });
    expect(result.current.route).toEqual({ page: "chat" });

    fireEvent.keyDown(window, { key: "ArrowRight", altKey: true });
    expect(result.current.route).toEqual({ page: "history" });
    expect(backDefault).toBe(false);
  });

  it("goes back with ⌘[ and forward with ⌘] on Apple platforms", () => {
    vi.mocked(isApplePlatform).mockReturnValue(true);
    const { result } = renderMidHistory();

    fireEvent.keyDown(window, { key: "ArrowLeft", altKey: true });
    expect(result.current.route).toEqual({ page: "history" });

    fireEvent.keyDown(window, { key: "[", metaKey: true });
    expect(result.current.route).toEqual({ page: "chat" });

    fireEvent.keyDown(window, { key: "]", metaKey: true });
    expect(result.current.route).toEqual({ page: "history" });
  });

  it("follows the keyboard's dedicated back and forward keys", () => {
    const { result } = renderMidHistory();

    fireEvent.keyDown(window, { key: "BrowserForward" });
    expect(result.current.route).toEqual({ page: "settings" });

    fireEvent.keyDown(window, { key: "BrowserBack" });
    expect(result.current.route).toEqual({ page: "history" });
  });

  it("follows the mouse's back and forward buttons and stops the webview handling them", () => {
    const { result } = renderMidHistory();

    const forwardDefault = fireEvent.mouseUp(window, { button: 4 });
    expect(result.current.route).toEqual({ page: "settings" });

    fireEvent.mouseUp(window, { button: 3 });
    fireEvent.mouseUp(window, { button: 3 });
    expect(result.current.route).toEqual({ page: "chat" });
    expect(forwardDefault).toBe(false);
  });

  it.each([
    ["an input", () => document.createElement("input")],
    ["a text area", () => document.createElement("textarea")],
    [
      "an editable element",
      () => {
        const element = document.createElement("div");
        element.setAttribute("contenteditable", "true");
        return element;
      },
    ],
  ])("leaves Alt+← to %s that has focus", (_case, create) => {
    const { result } = renderMidHistory();
    const field = document.body.appendChild(create());

    fireEvent.keyDown(field, { key: "ArrowLeft", altKey: true });

    expect(result.current.route).toEqual({ page: "history" });
    field.remove();
  });

  it.each([
    ["with Ctrl as well", { key: "ArrowLeft", altKey: true, ctrlKey: true }],
    ["with Shift as well", { key: "ArrowLeft", altKey: true, shiftKey: true }],
    ["as a held-down repeat", { key: "ArrowLeft", altKey: true, repeat: true }],
  ])("ignores Alt+← %s", (_case, init) => {
    const { result } = renderMidHistory();

    fireEvent.keyDown(window, init);

    expect(result.current.route).toEqual({ page: "history" });
  });

  it("ignores a shortcut another handler already consumed", () => {
    const { result } = renderMidHistory();
    const consume = (event: KeyboardEvent) => event.preventDefault();
    document.body.addEventListener("keydown", consume);

    fireEvent.keyDown(document.body, { key: "ArrowLeft", altKey: true });

    expect(result.current.route).toEqual({ page: "history" });
    document.body.removeEventListener("keydown", consume);
  });

  it("stops listening once unmounted", () => {
    const { result, unmount } = renderMidHistory();
    const before = result.current.route;

    unmount();
    fireEvent.keyDown(window, { key: "ArrowLeft", altKey: true });
    fireEvent.mouseUp(window, { button: 3 });

    expect(result.current.route).toEqual(before);
  });
});
