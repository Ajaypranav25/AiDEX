import { driveStatus, sandboxCapabilities } from "@aidex/client/desktop";
import type { HostProfile } from "@aidex/client/dto/host";
import type { BackendCapabilities } from "@aidex/client/dto/sandbox";
import type { DriveReport } from "@aidex/client/dto/status";
import { hostProfile } from "@aidex/client/host";
import { useAidexStatus } from "./useAidexStatus";
import { useBridgeQuery, type BridgeQuery } from "./useBridgeQuery";

/** `drive_status` for the session drive; refetched when the drive changes. Disabled without a drive. */
export function useDriveReport(): BridgeQuery<DriveReport> {
  const { driveRoot } = useAidexStatus();
  return useBridgeQuery(driveStatus, `drive|${driveRoot ?? ""}`, driveRoot !== null);
}

/** `host_profile`: read-only host facts. Needs no drive or daemon (outside the app it fails with `bridge_unavailable`). */
export function useHostProfile(): BridgeQuery<HostProfile> {
  return useBridgeQuery(hostProfile, "host");
}

/** `sandbox_capabilities`: what this host's sandbox backend enforces. Needs no drive or daemon. */
export function useSandboxCapabilities(): BridgeQuery<BackendCapabilities> {
  return useBridgeQuery(sandboxCapabilities, "sandbox");
}
