import { act, fireEvent, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
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

  it("treats a tabbed page's first tab and no tab as the same screen", () => {
    const { result } = renderNavigation();
    act(() => result.current.navigate({ page: "settings", tab: "appearance" }));

    act(() => result.current.navigate({ page: "settings" }));
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

  it("merges a replacement into the earlier entry it now repeats", () => {
    const { result } = renderNavigation();
    act(() => result.current.navigate({ page: "models", tab: "local" }));
    act(() => result.current.navigate({ page: "models", tab: "providers" }));

    act(() => result.current.replace({ page: "models", tab: "local" }));

    expect(result.current).toMatchObject({ route: { page: "models", tab: "local" }, canGoForward: false });
    act(() => result.current.back());
    expect(result.current.route).toEqual({ page: "chat" });
  });

  it("merges a replacement into the later entry it now repeats", () => {
    const { result } = renderNavigation();
    act(() => result.current.navigate({ page: "security", tab: "vault" }));
    act(() => result.current.navigate({ page: "settings" }));
    act(() => result.current.back());

    act(() => result.current.replace({ page: "settings" }));

    expect(result.current).toMatchObject({ route: { page: "settings" }, canGoBack: true, canGoForward: false });
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
  /** Elements and listeners a test adds, removed after it even when it fails. */
  const cleanups: (() => void)[] = [];
  afterEach(() => {
    for (const cleanup of cleanups.splice(0)) cleanup();
  });

  /** A provider that visited chat, history, and settings, then went back to history. */
  function renderMidHistory() {
    const rendered = renderNavigation();
    act(() => rendered.result.current.navigate({ page: "history" }));
    act(() => rendered.result.current.navigate({ page: "settings" }));
    act(() => rendered.result.current.back());
    return rendered;
  }

  /** Adds `element` to the page, types `text` into it when given, and focuses it. */
  function focusWith<E extends HTMLElement>(element: E, text?: string): E {
    document.body.appendChild(element);
    cleanups.push(() => element.remove());
    // Without `text` the control keeps its own value, such as a checkbox's "on".
    if (text !== undefined) {
      if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) element.value = text;
      else element.textContent = text;
    }
    element.focus();
    return element;
  }

  const input = (type = "text") => Object.assign(document.createElement("input"), { type });
  const editable = () => {
    const element = document.createElement("div");
    element.setAttribute("contenteditable", "true");
    element.tabIndex = 0;
    return element;
  };

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

    fireEvent.keyDown(window, { key: "[", metaKey: true });
    expect(result.current.route).toEqual({ page: "chat" });

    fireEvent.keyDown(window, { key: "]", metaKey: true });
    expect(result.current.route).toEqual({ page: "history" });
  });

  it("accepts ⌘[ typed with Option, which some layouts need for brackets", () => {
    vi.mocked(isApplePlatform).mockReturnValue(true);
    const { result } = renderMidHistory();

    fireEvent.keyDown(window, { key: "[", metaKey: true, altKey: true });

    expect(result.current.route).toEqual({ page: "chat" });
  });

  it("follows the keyboard's dedicated back and forward keys", () => {
    const { result } = renderMidHistory();

    fireEvent.keyDown(window, { key: "BrowserForward" });
    expect(result.current.route).toEqual({ page: "settings" });

    fireEvent.keyDown(window, { key: "BrowserBack" });
    expect(result.current.route).toEqual({ page: "history" });
  });

  it("follows the mouse's back and forward buttons", () => {
    const { result } = renderMidHistory();

    fireEvent.mouseUp(window, { button: 4 });
    expect(result.current.route).toEqual({ page: "settings" });

    fireEvent.mouseUp(window, { button: 3 });
    fireEvent.mouseUp(window, { button: 3 });
    expect(result.current.route).toEqual({ page: "chat" });
  });

  it("stops the webview acting on the side buttons and leaves other buttons alone", () => {
    renderMidHistory();

    expect(fireEvent.mouseDown(window, { button: 3 })).toBe(false);
    expect(fireEvent.mouseUp(window, { button: 4 })).toBe(false);
    expect(fireEvent.mouseDown(window, { button: 0 })).toBe(true);
    expect(fireEvent.mouseUp(window, { button: 0 })).toBe(true);
  });

  it.each([
    ["a text field", () => input()],
    ["a password field", () => input("password")],
    ["a text area", () => document.createElement("textarea")],
    ["an editable element", editable],
  ])("keeps a draft in %s on screen: no shortcut, key, or side button leaves it", (_case, create) => {
    const { result } = renderMidHistory();
    const field = focusWith(create(), "half-written message");

    fireEvent.keyDown(field, { key: "ArrowLeft", altKey: true });
    fireEvent.keyDown(field, { key: "BrowserBack" });
    const sideButtonDefault = fireEvent.mouseUp(window, { button: 3 });

    expect(result.current.route).toEqual({ page: "history" });
    expect(sideButtonDefault).toBe(false);
  });

  it.each([
    ["an empty text field", () => input(), ""],
    ["an empty text area", () => document.createElement("textarea"), ""],
    ["a checkbox", () => input("checkbox"), undefined],
    ["a radio button", () => input("radio"), undefined],
  ])("still goes back from %s, which holds no draft", (_case, create, text) => {
    const { result } = renderMidHistory();
    const field = focusWith(create(), text);

    fireEvent.keyDown(field, { key: "ArrowLeft", altKey: true });

    expect(result.current.route).toEqual({ page: "chat" });
  });

  it("waits while a modal dialog covers the page", () => {
    const { result } = renderMidHistory();
    const dialog = document.body.appendChild(document.createElement("dialog"));
    dialog.setAttribute("open", "");
    cleanups.push(() => dialog.remove());

    fireEvent.keyDown(window, { key: "ArrowLeft", altKey: true });
    fireEvent.mouseUp(window, { button: 3 });

    expect(result.current.route).toEqual({ page: "history" });
  });

  it.each([
    ["Alt+← with Ctrl as well", false, { key: "ArrowLeft", altKey: true, ctrlKey: true }],
    ["Alt+← with ⌘ as well", false, { key: "ArrowLeft", altKey: true, metaKey: true }],
    ["Alt+← with Shift as well", false, { key: "ArrowLeft", altKey: true, shiftKey: true }],
    ["a held-down Alt+←", false, { key: "ArrowLeft", altKey: true, repeat: true }],
    ["a held-down back key", false, { key: "BrowserBack", repeat: true }],
    ["Alt+← on an Apple platform", true, { key: "ArrowLeft", altKey: true }],
    ["⌘[ with Ctrl as well", true, { key: "[", metaKey: true, ctrlKey: true }],
    ["⌘[ with Shift as well", true, { key: "[", metaKey: true, shiftKey: true }],
  ])("ignores %s", (_case, apple, init) => {
    vi.mocked(isApplePlatform).mockReturnValue(apple);
    const { result } = renderMidHistory();

    fireEvent.keyDown(window, init);

    expect(result.current.route).toEqual({ page: "history" });
  });

  it.each([
    ["key", "keydown", () => fireEvent.keyDown(document.body, { key: "ArrowLeft", altKey: true })],
    ["side button", "mouseup", () => fireEvent.mouseUp(document.body, { button: 3 })],
  ])("ignores a back %s another handler already consumed", (_case, type, press) => {
    const { result } = renderMidHistory();
    const consume = (event: Event) => event.preventDefault();
    document.body.addEventListener(type, consume);
    cleanups.push(() => document.body.removeEventListener(type, consume));

    press();

    expect(result.current.route).toEqual({ page: "history" });
  });

  it("stops listening once unmounted", () => {
    const { unmount } = renderMidHistory();

    unmount();

    expect(fireEvent.keyDown(window, { key: "ArrowLeft", altKey: true })).toBe(true);
    expect(fireEvent.mouseDown(window, { button: 3 })).toBe(true);
    expect(fireEvent.mouseUp(window, { button: 3 })).toBe(true);
  });
});
