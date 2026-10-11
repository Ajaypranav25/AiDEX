import { useEffect, useRef, type ReactNode } from "react";
import { Brand, InfoCard, MediaItem, Notice } from "@aidex/ui/components";
import { FocusLayout } from "@aidex/ui/desktop";
import {
  AlertTriangleIcon,
  ArrowRightIcon,
  BoxIcon,
  CheckCircleIcon,
  CpuIcon,
  HardDriveIcon,
  LayersIcon,
  MemoryIcon,
  ShieldIcon,
  TerminalIcon,
} from "@aidex/ui/icons";
import { Badge, Button, Card, Container, Grid, Heading, ProgressBar, Stack, Text } from "@aidex/ui/primitives";
import { removableLabels } from "@aidex/client/dto/drive";
import { isAvailable, type HostProfile } from "@aidex/client/dto/host";
import { isActive, type BackendCapabilities } from "@aidex/client/dto/sandbox";
import type { DriveReport } from "@aidex/client/dto/status";
import type { AidexError } from "@aidex/client/errors";
import { formatBytes, UNKNOWN } from "@aidex/client/format";
import { useDriveReport, useHostProfile, useSandboxCapabilities } from "../../hooks/useCoreReports";
import { levelLabel, osLabel } from "../system/labels";

type Props = { onContinue: () => void };

type Check = { id: string; label: string; value: ReactNode; detail: ReactNode; status: "found" | "missing" | "unknown"; Icon: typeof CpuIcon };

const statusBadge = (status: Check["status"]) =>
  status === "found" ? (
    <Badge tone="success" icon={<CheckCircleIcon />}>Detected</Badge>
  ) : status === "missing" ? (
    <Badge tone="neutral" icon={<AlertTriangleIcon />}>Not found</Badge>
  ) : (
    <Badge tone="neutral">Unknown</Badge>
  );

const found = (names: string[]) => (names.length ? names.join(", ") : "None found");

function hostChecks(host: HostProfile | undefined): Check[] {
  if (!host) return [];
  const shells = host.shells.filter((tool) => isAvailable(tool.result)).map((tool) => tool.name);
  const tools = [
    isAvailable(host.git) ? "Git" : null,
    isAvailable(host.rust_toolchain.cargo) ? "Cargo" : null,
    isAvailable(host.rust_toolchain.rustc) ? "rustc" : null,
  ].filter((name): name is string => name !== null);
  const runtimes = host.inference_runtimes.filter((tool) => isAvailable(tool.result)).map((tool) => tool.name);
  return [
    {
      id: "system",
      label: "System",
      value: `${osLabel(host.os)} · ${host.architecture}`,
      detail: host.logical_cpu_count ? `${host.logical_cpu_count} logical CPUs` : "CPU count unknown",
      status: "found",
      Icon: CpuIcon,
    },
    {
      id: "memory",
      label: "Memory",
      value: host.ram_bytes === null ? UNKNOWN : formatBytes(host.ram_bytes),
      detail: host.ram_bytes === null ? "Not reported on this platform yet" : "Physical memory",
      status: host.ram_bytes === null ? "unknown" : "found",
      Icon: MemoryIcon,
    },
    {
      id: "tools",
      label: "Developer tools",
      value: found(tools),
      detail: `Shells: ${found(shells)}`,
      status: tools.length ? "found" : "missing",
      Icon: TerminalIcon,
    },
    {
      id: "runtimes",
      label: "Local model runtimes",
      value: found(runtimes),
      detail: "Searched on PATH (ollama, llama-server, llama-cli)",
      status: runtimes.length ? "found" : "missing",
      Icon: BoxIcon,
    },
  ];
}

function sandboxCheck(sandbox: BackendCapabilities | undefined): Check[] {
  if (!sandbox) return [];
  const active = sandbox.capabilities.filter((capability) => isActive(capability.enforcement)).length;
  return [
    {
      id: "sandbox",
      label: "Sandbox",
      value: sandbox.max_level ? levelLabel(sandbox.max_level) : "Cannot launch processes",
      detail:
        sandbox.helper.state === "missing"
          ? "The sandbox helper is missing next to AiDEX"
          : `${active} of ${sandbox.capabilities.length} protections applied on this host`,
      status: sandbox.max_level ? "found" : "missing",
      Icon: ShieldIcon,
    },
  ];
}

function driveCheck(report: DriveReport | undefined): Check[] {
  if (!report) return [];
  const health = report.health;
  return [
    {
      id: "drive",
      label: "Drive",
      value: report.volume?.label ?? report.root,
      detail: health
        ? `Schema v${health.schema_version ?? "?"} · ${formatBytes(health.free_bytes)} free${report.volume ? ` · ${removableLabels[report.volume.removable]}` : ""}`
        : (report.health_error?.message ?? UNKNOWN),
      status: health ? "found" : "missing",
      Icon: HardDriveIcon,
    },
  ];
}

/** Live host, sandbox, and drive detection before entering the workspace. */
export function RuntimeDetectionPage({ onContinue }: Props) {
  const host = useHostProfile();
  const sandbox = useSandboxCapabilities();
  const drive = useDriveReport();
  const headingRef = useRef<HTMLHeadingElement>(null);

  // The button that led here has unmounted; land focus on this screen's title.
  useEffect(() => headingRef.current?.focus(), []);

  const queries = [host, sandbox, ...(drive.loading || drive.settled ? [drive] : [])];
  const completed = queries.filter((query) => query.settled).length;
  const checks = [...hostChecks(host.data), ...sandboxCheck(sandbox.data), ...driveCheck(drive.data)];
  const failures = [host.error, sandbox.error, drive.error].filter((error): error is AidexError => error !== null);
  const migration = drive.data?.health?.migration_advice ?? null;
  const done = completed === queries.length;

  return (
    <FocusLayout>
      <Container size="lg" verticallyCentered>
        <Stack gap={10}>
          <Stack gap={8} align="center">
            <Brand size="xl" />
            <Stack gap={3}>
              <Heading ref={headingRef} tabIndex={-1} level={1} size="display" align="center">
                Host &amp; Runtime Detection
              </Heading>
              <Text variant="lead" measure="wide" tone="secondary" align="center">
                Read-only checks of this computer, its sandbox support, and your AiDEX drive.
              </Text>
            </Stack>
          </Stack>
          <Container size="md">
            <ProgressBar label="Detection progress" value={completed} max={queries.length} showCount />
          </Container>
          {checks.length ? (
            <Grid columns={3} gap={4}>
              {checks.map(({ id, label, value, detail, status, Icon }) => (
                <InfoCard key={id} icon={<Icon />} status={statusBadge(status)} label={label} value={value} detail={detail} />
              ))}
            </Grid>
          ) : null}
          <Container size="md">
            <Stack gap={4}>
              {failures.map((error) => (
                <Notice key={error.kind + error.message} tone="warning">
                  {error.message}
                </Notice>
              ))}
              {migration ? <Notice tone="warning">Drive migration available: {migration}</Notice> : null}
              <Card>
                <MediaItem
                  media={<LayersIcon size={36} />}
                  title="Runtime summary"
                  titleLevel={2}
                  size="lg"
                  description={
                    !done
                      ? "Detecting…"
                      : sandbox.data?.max_level
                        ? `Agent processes can run at up to ${levelLabel(sandbox.data.max_level)} on this computer. Requests for stronger isolation are refused rather than weakened.`
                        : "Detection finished. Values that could not be read are shown as Unknown."
                  }
                />
              </Card>
            </Stack>
          </Container>
          <Stack direction="row" justify="center">
            <Button variant="primary" size="xl" shape="pill" trailing={<ArrowRightIcon />} onClick={onContinue}>
              Continue to AiDEX
            </Button>
          </Stack>
        </Stack>
      </Container>
    </FocusLayout>
  );
}
