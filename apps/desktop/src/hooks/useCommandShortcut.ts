import { useEffect } from "react";
import { isApplePlatform } from "../lib/platform";

/** Runs `handler` on ⌘+`key` (Apple platforms) or Ctrl+`key` (elsewhere). Pass a stable handler. */
export function useCommandShortcut(key: string, handler: () => void) {
  useEffect(() => {
    const expectedKey = key.toLowerCase();
    const appleModifier = isApplePlatform();
    const handleKeyDown = (event: KeyboardEvent) => {
      const commandPressed = appleModifier ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
      if (!commandPressed || event.altKey || event.shiftKey || event.repeat) return;
      // Synthetic events (e.g. autofill) can arrive without a key.
      if (event.key?.toLowerCase() !== expectedKey) return;
      event.preventDefault();
      handler();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [key, handler]);
}
