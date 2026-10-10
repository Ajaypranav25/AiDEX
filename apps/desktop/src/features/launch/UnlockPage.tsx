import { useEffect, useRef } from "react";
import { Brand, MediaItem } from "@aidex/ui/components";
import { FocusLayout } from "@aidex/ui/desktop";
import { ArrowRightIcon, HardDriveIcon, LockIcon, LockOpenIcon } from "@aidex/ui/icons";
import { Button, Card, Container, Heading, IconBadge, Stack, StatusIndicator, Text } from "@aidex/ui/primitives";
import { removableLabels } from "@aidex/client/dto/drive";
import { ErrorKind } from "@aidex/client/errors";
import { formatBytes } from "@aidex/client/format";
import { callUntyped } from "@aidex/client/rpc";
import { useAidexStatus } from "../../hooks/useAidexStatus";
import { useBridgeQuery } from "../../hooks/useBridgeQuery";
import { useDriveReport } from "../../hooks/useCoreReports";
import { CoreStatusNotice } from "../system/CoreStatusNotice";
import { driveSourceLabels } from "../system/labels";

type Props = { onContinue: () => void };

type VaultView = { title: string; description: string; tone: "success" | "warning" | "neutral"; label: string };

/**
 * Drive and vault check before the workspace. The vault backend (crates/vault, issue #13) is not
 * part of this build, so this page reports that honestly instead of asking for a password it
 * cannot verify. `UnlockForm` holds the password UI for when the vault reports a locked state.
 */
export function UnlockPage({ onContinue }: Props) {
  const { status, connectionKey, daemonRunning } = useAidexStatus();
  const drive = useDriveReport();
  const vault = useBridgeQuery(() => callUntyped("vault.status"), `vault|${connectionKey}`, daemonRunning);
  const headingRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => headingRef.current?.focus(), []);

  const resolved = status?.drive.state === "resolved" ? status.drive : null;
  const volume = drive.data?.volume ?? null;
  const driveTitle = volume?.label ?? resolved?.root ?? "No drive selected";
  const driveDetail = resolved
    ? [resolved.root, volume ? removableLabels[volume.removable] : null, volume ? formatBytes(volume.total_bytes) : null, driveSourceLabels[resolved.source]]
        .filter(Boolean)
        .join(" · ")
    : "Screens show sample data until a drive is chosen.";

  const vaultView: VaultView = !daemonRunning
    ? {
        title: "Vault status unavailable",
        description: "AiDEX Core is not running, so the vault on this drive cannot be checked. Nothing is unlocked.",
        tone: "neutral",
        label: "Unknown",
      }
    : vault.error?.kind === ErrorKind.methodNotFound
      ? {
          title: "Vault not set up on this drive",
          description:
            "This AiDEX build has no encrypted vault yet, so there is nothing to unlock and no password is asked for. AiDEX stores no secrets until the vault is installed.",
          tone: "warning",
          label: "Not set up",
        }
      : vault.data !== undefined
        ? {
            title: "Vault found",
            description: "Unlocking from the desktop arrives with the vault update; this build does not unlock it.",
            tone: "neutral",
            label: "Locked",
          }
        : vault.error
          ? { title: "Vault status unavailable", description: vault.error.message, tone: "neutral", label: "Unknown" }
          : { title: "Checking the vault…", description: "Asking AiDEX Core about this drive's vault.", tone: "neutral", label: "Checking" };

  return (
    <FocusLayout>
      <Container size="xs" verticallyCentered>
        <Stack gap={10}>
          <Stack gap={2} align="center">
            <Brand size="xl" layout="stacked" />
            <Heading ref={headingRef} tabIndex={-1} level={1} size="title" align="center">
              Drive and vault
            </Heading>
            <Text variant="lead" tone="secondary" align="center">
              Your portable AI workspace
            </Text>
          </Stack>
          <Stack gap={6}>
            <Card padding="md">
              <MediaItem
                media={<IconBadge icon={<HardDriveIcon />} />}
                title={driveTitle}
                description={driveDetail}
                trailing={<StatusIndicator tone={resolved ? "success" : "warning"} label={resolved ? "Detected" : "No drive"} />}
              />
            </Card>
            {resolved ? <CoreStatusNotice requires="daemon" /> : null}
            <Card>
              <Stack gap={7}>
                <MediaItem
                  media={<IconBadge tone="accent" icon={vaultView.label === "Locked" ? <LockIcon /> : <LockOpenIcon />} />}
                  title={vaultView.title}
                  titleLevel={2}
                  description={vaultView.description}
                  trailing={<StatusIndicator tone={vaultView.tone} label={vaultView.label} />}
                />
                <Button variant="tonal" size="lg" align="center" fullWidth trailing={<ArrowRightIcon />} onClick={onContinue}>
                  Continue
                </Button>
              </Stack>
            </Card>
          </Stack>
        </Stack>
      </Container>
    </FocusLayout>
  );
}
