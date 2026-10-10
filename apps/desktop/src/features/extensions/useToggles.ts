import { useMemo, useState } from "react";

export type Toggles = {
  isOn: (id: string) => boolean;
  set: (id: string, on: boolean) => void;
};

/** In-memory on/off state for a set of items, seeded from sample data; nothing is written to the drive. */
export function useToggles(items: readonly { id: string; enabled?: boolean }[]): Toggles {
  const [state, setState] = useState<Readonly<Record<string, boolean>>>(() =>
    Object.fromEntries(items.map((item) => [item.id, item.enabled ?? false])),
  );

  return useMemo(
    () => ({
      isOn: (id: string) => state[id] ?? false,
      set: (id: string, on: boolean) => setState((current) => ({ ...current, [id]: on })),
    }),
    [state],
  );
}
