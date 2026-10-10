import { SectionHeader, StatTile } from "@aidex/ui/components";
import { CpuIcon, HardDriveIcon, ServerIcon } from "@aidex/ui/icons";
import { Grid, Stack } from "@aidex/ui/primitives";
import { removableLabels } from "@aidex/client/dto/drive";
import { formatBytes, formatDuration } from "@aidex/client/format";
import { useAidexStatus } from "../../hooks/useAidexStatus";
import { useDriveReport, useHostProfile, useSandboxCapabilities } from "../../hooks/useCoreReports";
import { CoreStatusNotice } from "../system/CoreStatusNotice";
import { LiveEvents } from "../system/LiveEvents";
import { levelLabel, osLabel } from "../system/labels";
import { WorkspaceList } from "./WorkspaceList";

/** Live AiDEX Core, drive, host, workspace, and event summary on Home. Outside the app it only explains why nothing is live. */
export function SystemOverview() {
  const { status, daemon, driveRoot, bridgeMissing } = useAidexStatus();
  const drive = useDriveReport();
  const host = useHostProfile();
  const sandbox = useSandboxCapabilities();

  if (bridgeMissing) return <CoreStatusNotice requires="daemon" />;

  const health = drive.data?.health ?? null;
  const volume = drive.data?.volume ?? null;
  const daemonHint = daemon
    ? `Up ${formatDuration(daemon.uptime_ms)} · pid ${daemon.pid} · IPC v${daemon.contract_version}`
    : status?.daemon.state === "no_drive"
      ? "Needs a drive"
      : "Start it to use live features";
  const driveHint = !driveRoot
    ? "Choose a drive to work from"
    : health
      ? [
          `Schema v${health.schema_version ?? "?"}`,
          `${formatBytes(health.free_bytes)} free`,
          volume ? removableLabels[volume.removable] : null,
          health.migration_required ? "migration available" : null,
        ]
          .filter(Boolean)
          .join(" · ")
      : (drive.data?.health_error?.message ?? (drive.loading ? "Checking…" : "Unknown"));
  const hostValue = host.data ? `${osLabel(host.data.os)} · ${host.data.architecture}` : host.loading ? "Checking…" : "Unknown";
  const hostHint = [
    host.data?.logical_cpu_count ? `${host.data.logical_cpu_count} logical CPUs` : null,
    sandbox.data ? `Sandbox: ${sandbox.data.max_level ? levelLabel(sandbox.data.max_level) : "unavailable"}` : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <section>
      <Stack gap={4}>
        <SectionHeader title="System status" />
        <CoreStatusNotice requires="daemon" />
        <Grid columns={3} gap={4}>
          <StatTile
            label="AiDEX Core"
            value={daemon ? "Running" : "Not running"}
            hint={daemonHint}
            icon={<ServerIcon />}
            tone={daemon ? "success" : "warning"}
          />
          <StatTile
            label="Drive"
            value={volume?.label ?? driveRoot ?? "No drive"}
            hint={driveHint}
            icon={<HardDriveIcon />}
            tone={health && !health.migration_required ? "success" : "warning"}
          />
          <StatTile label="This computer" value={hostValue} hint={hostHint || undefined} icon={<CpuIcon />} />
        </Grid>
        {daemon ? (
          <Grid columns={2} gap={6}>
            <Stack gap={3}>
              <SectionHeader title="Workspaces" level={3} />
              <WorkspaceList />
            </Stack>
            <Stack gap={3}>
              <SectionHeader title="Live activity" level={3} meta="events.subscribe" />
              <LiveEvents />
            </Stack>
          </Grid>
        ) : null}
      </Stack>
    </section>
  );
}
