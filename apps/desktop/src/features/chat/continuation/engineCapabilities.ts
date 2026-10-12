import { useEffect, useState } from "react";
import { callUntyped } from "@aidex/client/rpc";

// One answer per window: the engine runs on this computer and its sandbox does not change while the
// app is open.
let known: boolean | undefined;

/** The webview's own platform; the daemon runs on the same computer. */
function windowsHost(): boolean {
  return typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
}

/**
 * Whether shell commands outside Full access are refused, from the engine capabilities in
 * `execution.status` (`shell_requires_full_access_on_windows`, or no filesystem confinement for
 * commands on this host). Undefined when the result does not say.
 */
export function shellNeedsFullAccessFrom(status: unknown): boolean | undefined {
  if (typeof status !== "object" || status === null) return undefined;
  const capabilities = (status as Record<string, unknown>).capabilities;
  if (typeof capabilities !== "object" || capabilities === null) return undefined;
  const windows = (capabilities as Record<string, unknown>).shell_requires_full_access_on_windows;
  const confinement = (capabilities as Record<string, unknown>).filesystem_confinement;
  if (typeof windows !== "boolean" && typeof confinement !== "boolean") return undefined;
  return windows === true || confinement === false;
}

/**
 * Whether commands need Full access on this computer, for honest permission descriptions. Asks the
 * engine once `enabled` (a permission choice is on screen); until it answers, assumes so on Windows.
 */
export function useShellNeedsFullAccess(enabled: boolean): boolean {
  const [value, setValue] = useState(() => known ?? windowsHost());
  useEffect(() => {
    if (!enabled) return;
    if (known !== undefined) {
      setValue(known);
      return;
    }
    let live = true;
    callUntyped("execution.status")
      .then((status) => {
        const answer = shellNeedsFullAccessFrom(status);
        if (answer === undefined) return;
        known = answer;
        if (live) setValue(answer);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [enabled]);
  return value;
}
