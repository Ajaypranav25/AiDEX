import { useApplePlatform } from "./useApplePlatform";

/** Label of the platform's command modifier: "⌘" on Apple platforms, "Ctrl" elsewhere. */
export function useModifierKeyLabel(): string {
  return useApplePlatform() ? "⌘" : "Ctrl";
}
