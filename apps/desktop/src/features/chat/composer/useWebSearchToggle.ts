import { useCallback, useState } from "react";
import type { WebSettingsView } from "@aidex/client/dto/web";

// The per-conversation Chat web search switch. It is local to this window (never stored in the
// session configuration) but outlives the page remount that follows a new chat's first send.
const switches = new Map<string, boolean>();

export type WebSearchAvailability = {
  /** Web search is switched on and configured, so the daemon accepts `web_search`. */
  available: boolean;
  /** Why it is not, for the toggle's tooltip. */
  reason?: string;
};

/** Whether the web settings let a run search the web. */
export function webSearchAvailability(settings: WebSettingsView | undefined): WebSearchAvailability {
  if (settings?.enabled && settings.configured) return { available: true };
  return { available: false, reason: "Set up web search in Extensions → Web" };
}

/**
 * The Chat web search switch of `conversationId`; a new chat (no id) starts with it off. `adopt`
 * carries a new chat's choice over to the conversation its first send created.
 */
export function useWebSearchToggle(conversationId: string | undefined) {
  const [on, setOn] = useState(() => (conversationId ? (switches.get(conversationId) ?? false) : false));
  const set = useCallback(
    (value: boolean) => {
      if (conversationId) switches.set(conversationId, value);
      setOn(value);
    },
    [conversationId],
  );
  const adopt = useCallback((createdId: string) => switches.set(createdId, on), [on]);
  return { on, set, adopt };
}
