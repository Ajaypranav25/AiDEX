import { useSyncExternalStore } from "react";
import { isApplePlatform } from "../lib/platform";

const subscribe = () => () => {};
const getClientLabel = () => (isApplePlatform() ? "⌘" : "Ctrl");
const getPrerenderLabel = () => "Ctrl";

/** Label of the platform's command modifier: "⌘" on Apple platforms, "Ctrl" elsewhere. */
export function useModifierKeyLabel(): string {
  return useSyncExternalStore(subscribe, getClientLabel, getPrerenderLabel);
}
