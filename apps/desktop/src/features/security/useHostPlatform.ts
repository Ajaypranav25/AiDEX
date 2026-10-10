import { useSyncExternalStore } from "react";
import type { SandboxBackend } from "./sample";

type HostPlatform = SandboxBackend["id"] | null;

const subscribe = () => () => {};

function detectPlatform(): HostPlatform {
  const agent = navigator.userAgent;
  if (/Windows/i.test(agent)) return "windows";
  if (/Mac/i.test(agent)) return "macos";
  if (/Linux|X11/i.test(agent)) return "linux";
  return null;
}

/** The sandbox backend family for this host, or null while prerendering or when unknown. */
export function useHostPlatform(): HostPlatform {
  return useSyncExternalStore(subscribe, detectPlatform, () => null);
}
