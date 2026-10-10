import { useState, type FormEvent, type ReactNode } from "react";
import { KeyValueList, List, ListRow, SectionHeader } from "@aidex/ui/components";
import { DownloadIcon, PlayIcon, SearchIcon, StethoscopeIcon } from "@aidex/ui/icons";
import { Button, Stack, StatusIndicator, Text, TextField } from "@aidex/ui/primitives";
import { removableLabels } from "@aidex/client/dto/drive";
import { formatBytes, formatDuration, UNKNOWN } from "@aidex/client/format";
import { useToast } from "../../app/providers/toast";
import { useAidexStatus } from "../../hooks/useAidexStatus";
import { useDriveReport, useHostProfile, useSandboxCapabilities } from "../../hooks/useCoreReports";
import { CoreStatusNotice } from "../system/CoreStatusNotice";
import { levelLabel, osLabel } from "../system/labels";
import { RecentEvents } from "../system/RecentEvents";

type Check = { id: string; title: string; detail: string; tone: "success" | "warning" | "neutral"; status: string; action?: ReactNode };

/** Doctor-equivalent checks (host, drive, AiDEX Core, sandbox), system details, and an event log reader. */
export function DiagnosticsPanel() {
  const notify = useToast();
  const aidex = useAidexStatus();
  const host = useHostProfile();
  const sandbox = useSandboxCapabilities();
  const drive = useDriveReport();
  const { status, daemon } = aidex;

  const rerun = () => {
    void aidex.refresh();
    host.refetch();
    sandbox.refetch();
    drive.refetch();
  };

  const health = drive.data?.health ?? null;
  const checks: Check[] = [
    host.data
      ? { id: "host", title: "Host detected", detail: `${osLabel(host.data.os)} · ${host.data.architecture}`, tone: "success", status: "Passed" }
      : { id: "host", title: "Host detection", detail: host.error?.message ?? "Checking…", tone: host.error ? "warning" : "neutral", status: host.error ? "Failed" : "Running" },
    status?.drive.state !== "resolved"
      ? {
          id: "drive",
          title: "Drive",
          detail: status?.drive.state === "invalid" ? status.drive.error.message : "No AiDEX drive selected for this session",
          tone: "warning",
          status: "Not available",
        }
      : health
        ? {
            id: "drive",
            title: health.migration_required ? "Drive manifest needs migration" : "Drive manifest valid",
            detail: `Schema v${health.schema_version ?? "?"} of v${health.current_schema_version} · ${health.writable ? "writable" : "read-only"}`,
            tone: health.migration_required || !health.writable ? "warning" : "success",
            status: health.migration_required ? "Migrate" : health.writable ? "Passed" : "Read-only",
          }
        : {
            id: "drive",
            title: "Drive",
            detail: drive.data?.health_error?.message ?? drive.error?.message ?? "Checking…",
            tone: drive.loading ? "neutral" : "warning",
            status: drive.loading ? "Running" : "Failed",
          },
    daemon
      ? {
          id: "daemon",
          title: "AiDEX Core running",
          detail: `aidexd ${daemon.version} · pid ${daemon.pid} · up ${formatDuration(daemon.uptime_ms)} · IPC v${daemon.contract_version}`,
          tone: "success",
          status: "Passed",
        }
      : {
          id: "daemon",
          title: "AiDEX Core",
          detail: status?.daemon.state === "no_drive" || !status ? "Needs a drive" : "error" in status.daemon ? status.daemon.error.message : UNKNOWN,
          tone: "warning",
          status: status?.daemon.state === "stopped" ? "Stopped" : "Unavailable",
          action:
            status?.daemon.state === "stopped" ? (
              <Button variant="outline" size="md" icon={<PlayIcon />} disabled={aidex.pending !== null} onClick={() => void aidex.startDaemon()}>
                {aidex.pending === "start" ? "Starting…" : "Start"}
              </Button>
            ) : undefined,
        },
    sandbox.data
      ? {
          id: "sandbox",
          title: sandbox.data.max_level ? "Sandbox backend available" : "Sandbox cannot launch processes",
          detail:
            sandbox.data.helper.state === "missing"
              ? `Helper missing: ${sandbox.data.helper.reason}`
              : `${sandbox.data.backend.replace(/_/g, " ")} · up to ${sandbox.data.max_level ? levelLabel(sandbox.data.max_level) : "no level"}`,
          tone: sandbox.data.max_level ? "success" : "warning",
          status: sandbox.data.max_level ? "Passed" : "Warning",
        }
      : {
          id: "sandbox",
          title: "Sandbox probe",
          detail: sandbox.error?.message ?? "Checking…",
          tone: sandbox.error ? "warning" : "neutral",
          status: sandbox.error ? "Failed" : "Running",
        },
  ];
  const passed = checks.filter((check) => check.tone === "success").length;
  const warnings = checks.filter((check) => check.tone === "warning").length;

  const volume = drive.data?.volume ?? null;
  const systemInfo = [
    { label: "AiDEX desktop", value: status?.app_version ?? UNKNOWN },
    { label: "AiDEX Core", value: daemon ? `aidexd ${daemon.version} (pid ${daemon.pid})` : "Not running" },
    { label: "Operating system", value: host.data ? `${osLabel(host.data.os)} · ${host.data.architecture}` : UNKNOWN },
    { label: "Logical CPUs", value: host.data?.logical_cpu_count ?? UNKNOWN },
    { label: "Memory", value: formatBytes(host.data?.ram_bytes) },
    {
      label: "Drive",
      value: aidex.driveRoot
        ? [aidex.driveRoot, volume?.filesystem, volume ? removableLabels[volume.removable] : null].filter(Boolean).join(" · ")
        : "None",
    },
  ];

  return (
    <Stack gap={10}>
      <CoreStatusNotice requires="drive" />
      <section>
        <SectionHeader
          title="Health checks"
          meta={`${passed} passed · ${warnings} ${warnings === 1 ? "warning" : "warnings"}`}
          actions={
            <Stack direction="row" gap={2}>
              <Button variant="outline" size="md" icon={<StethoscopeIcon />} onClick={rerun}>
                Run diagnostics
              </Button>
              <Button
                variant="ghost"
                size="md"
                icon={<DownloadIcon />}
                onClick={() => notify("Preview: exporting logs will be available when the logging service is part of AiDEX Core.")}
              >
                Export logs
              </Button>
            </Stack>
          }
        />
        <List>
          {checks.map((check) => (
            <ListRow
              key={check.id}
              title={check.title}
              subtitle={check.detail}
              trailing={
                <Stack direction="row" gap={3} align="center">
                  {check.action}
                  <StatusIndicator tone={check.tone} label={check.status} />
                </Stack>
              }
            />
          ))}
        </List>
      </section>
      <section>
        <SectionHeader title="System information" />
        <KeyValueList items={systemInfo} />
      </section>
      <EventLogReader />
    </Stack>
  );
}

/** Reads a workspace's durable events, like `aidex events --workspace <id>`. */
function EventLogReader() {
  const { daemonRunning } = useAidexStatus();
  const [draft, setDraft] = useState("");
  const [workspaceId, setWorkspaceId] = useState<string | null>(null);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setWorkspaceId(draft.trim() || null);
  };

  return (
    <section>
      <Stack gap={4}>
        <SectionHeader title="Event log" meta="events.replay" />
        <Text tone="secondary">Durable events are kept per workspace. Enter a workspace id to read its latest events.</Text>
        <form onSubmit={submit} noValidate>
          <Stack direction="row" gap={2} align="end">
            <TextField
              label="Workspace id"
              size="md"
              spellCheck={false}
              autoComplete="off"
              value={draft}
              disabled={!daemonRunning}
              hint={daemonRunning ? undefined : "Start AiDEX Core to read events."}
              onChange={(event) => setDraft(event.target.value)}
            />
            <Button type="submit" variant="outline" size="md" icon={<SearchIcon />} disabled={!daemonRunning || !draft.trim()}>
              Read events
            </Button>
          </Stack>
        </form>
        {workspaceId && daemonRunning ? <RecentEvents workspaceId={workspaceId} /> : null}
      </Stack>
    </section>
  );
}
