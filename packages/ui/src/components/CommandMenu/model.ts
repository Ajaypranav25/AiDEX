// Pure helpers for `CommandMenu`: no DOM or React, so they run during prerendering and in unit tests.

import type { CommandMenuItem, CommandMenuSection } from "./CommandMenu";

/**
 * DOM id of a row's option element, for the field's `aria-activedescendant`. Characters outside
 * `[A-Za-z0-9_-]` are hex-escaped, so any item id (including lone surrogates) maps to a unique id.
 */
export function commandMenuOptionId(listId: string, itemId: string): string {
  let safe = "";
  for (const char of Array.from(itemId)) {
    safe += /^[A-Za-z0-9_-]$/.test(char) ? char : `.${(char.codePointAt(0) ?? 0).toString(16)}.`;
  }
  return `${listId}-option-${safe}`;
}

/** Rows of every section in display order. */
export function flattenCommandMenu(sections: readonly CommandMenuSection[]): CommandMenuItem[] {
  const items: CommandMenuItem[] = [];
  for (const section of sections) items.push(...section.items);
  return items;
}

/** Splits `text` around the first case-insensitive occurrence of `query` (empty match when none). */
export function highlightMatch(text: string, query: string): [string, string, string] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [text, "", ""];
  const index = text.toLowerCase().indexOf(needle);
  // Lower-casing can change the length of some characters; only highlight when offsets line up.
  if (index === -1 || text.toLowerCase().length !== text.length) return [text, "", ""];
  return [text.slice(0, index), text.slice(index, index + needle.length), text.slice(index + needle.length)];
}

export type CommandMenuFieldState = {
  /** The menu's `id`. */
  listId: string;
  open: boolean;
  /** The menu's `activeId`. */
  activeId: string | null;
};

/** ARIA attributes that tie a text field to an open `CommandMenu`; spread them onto the field. */
export function commandMenuFieldProps({ listId, open, activeId }: CommandMenuFieldState) {
  return {
    "aria-autocomplete": "list" as const,
    "aria-haspopup": "listbox" as const,
    "aria-controls": open ? listId : undefined,
    "aria-activedescendant": open && activeId !== null ? commandMenuOptionId(listId, activeId) : undefined,
  };
}
