import { useEffect } from "react";
import { isApplePlatform } from "../lib/platform";

/** `MouseEvent.button` of the mouse's back and forward side buttons. */
const MOUSE_BACK = 3;
const MOUSE_FORWARD = 4;

/** Input types whose value is text someone typed. */
const TEXT_INPUT_TYPES = new Set(["text", "search", "email", "url", "tel", "password", "number"]);

type Step = "back" | "forward";

/** How the shortcuts below read on each platform, for tooltips and `aria-keyshortcuts`. */
export const historyShortcutLabels = {
  apple: { back: { label: "⌘[", aria: "Meta+[" }, forward: { label: "⌘]", aria: "Meta+]" } },
  other: { back: { label: "Alt+←", aria: "Alt+ArrowLeft" }, forward: { label: "Alt+→", aria: "Alt+ArrowRight" } },
} as const satisfies Record<"apple" | "other", Record<Step, { label: string; aria: string }>>;

/**
 * Whether the focused field holds typed text. A history step remounts the page and drops such a
 * draft, so shortcuts, keys, and side buttons wait until the field is empty or loses focus.
 */
function focusHoldsDraft(): boolean {
  const focused = document.activeElement;
  if (focused instanceof HTMLTextAreaElement) return focused.value !== "";
  if (focused instanceof HTMLInputElement) return TEXT_INPUT_TYPES.has(focused.type) && focused.value !== "";
  const editable = focused?.closest("[contenteditable='true'], [contenteditable='']");
  return Boolean(editable?.textContent?.trim());
}

/** A modal dialog makes the page behind it inert, so its history must not change underneath. */
const modalOpen = () => document.querySelector("dialog[open]") !== null;

function stepForKey(event: KeyboardEvent, apple: boolean): Step | null {
  if (event.repeat || event.shiftKey) return null;
  if (event.key === "BrowserBack") return "back";
  if (event.key === "BrowserForward") return "forward";
  if (apple) {
    // Option stays allowed: some layouts need it to type a bracket at all.
    if (!event.metaKey || event.ctrlKey) return null;
    return event.key === "[" ? "back" : event.key === "]" ? "forward" : null;
  }
  if (!event.altKey || event.ctrlKey || event.metaKey) return null;
  return event.key === "ArrowLeft" ? "back" : event.key === "ArrowRight" ? "forward" : null;
}

function stepForButton(button: number): Step | null {
  return button === MOUSE_BACK ? "back" : button === MOUSE_FORWARD ? "forward" : null;
}

/**
 * Walks the in-app history with the platform's back and forward keys (Alt+← and Alt+→, or ⌘[
 * and ⌘] on Apple platforms), the keyboard's dedicated back and forward keys, and the mouse's
 * side buttons. The webview's own handling of the side buttons is always suppressed. Pass stable
 * callbacks.
 */
export function useHistoryShortcuts(back: () => void, forward: () => void) {
  useEffect(() => {
    const apple = isApplePlatform();
    const go = (step: Step) => (step === "back" ? back() : forward());
    const blocked = () => modalOpen() || focusHoldsDraft();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const step = stepForKey(event, apple);
      if (!step || blocked()) return;
      event.preventDefault();
      go(step);
    };
    const suppressButton = (event: MouseEvent) => {
      if (stepForButton(event.button)) event.preventDefault();
    };
    const handleMouseUp = (event: MouseEvent) => {
      if (event.defaultPrevented) return;
      const step = stepForButton(event.button);
      if (!step) return;
      event.preventDefault();
      if (!blocked()) go(step);
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("mousedown", suppressButton);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("mousedown", suppressButton);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [back, forward]);
}
