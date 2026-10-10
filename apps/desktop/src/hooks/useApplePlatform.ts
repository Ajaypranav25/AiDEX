import { useSyncExternalStore } from "react";
import { isApplePlatform } from "../lib/platform";

const subscribe = () => () => {};
const getPrerenderValue = () => false;

/** True on macOS and iOS. Prerendered markup assumes another platform until hydration. */
export function useApplePlatform(): boolean {
  return useSyncExternalStore(subscribe, isApplePlatform, getPrerenderValue);
}
