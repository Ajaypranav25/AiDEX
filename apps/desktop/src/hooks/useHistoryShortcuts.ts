import { useEffect } from "react";
import { isApplePlatform } from "../lib/platform";

/** `MouseEvent.button` of the mouse's back and forward side buttons. */
const MOUSE_BACK = 3;
const MOUSE_FORWARD = 4;

/** Text entry keeps Alt+← and ⌘[ to itself, so a shortcut never discards a draft. */
const EDITABLE = "input, textarea, select, [contenteditable='true'], [contenteditable='']";

type Step = "back" | "forward";

/** How the shortcuts below read on each platform, for tooltips and `aria-keyshortcuts`. */
export const historyShortcutLabels = {
  apple: { back: { label: "⌘[", aria: "Meta+[" }, forward: { label: "⌘]", aria: "Meta+]" } },
  other: { back: { label: "Alt+←", aria: "Alt+ArrowLeft" }, forward: { label: "Alt+→", aria: "Alt+ArrowRight" } },
} as const satisfies Record<"apple" | "other", Record<Step, { label: string; aria: string }>>;

function stepForKey(event: KeyboardEvent, apple: boolean): Step | null {
  if (event.key === "BrowserBack") return "back";
  if (event.key === "BrowserForward") return "forward";
  if (event.shiftKey || event.repeat) return null;
  if (event.target instanceof Element && event.target.closest(EDITABLE)) return null;
  if (apple) {
    if (!event.metaKey || event.ctrlKey || event.altKey) return null;
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
 * side buttons. The webview's own history handling of those buttons is suppressed.
 */
export function useHistoryShortcuts(back: () => void, forward: () => void) {
  useEffect(() => {
    const apple = isApplePlatform();
    const go = (step: Step) => (step === "back" ? back() : forward());

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const step = stepForKey(event, apple);
      if (!step) return;
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
      go(step);
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
