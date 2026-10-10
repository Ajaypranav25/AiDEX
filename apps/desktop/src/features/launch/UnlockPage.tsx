import { useEffect, useRef, useState } from "react";
import { Brand, CodeBlock, MediaItem, Notice } from "@aidex/ui/components";
import { FocusLayout } from "@aidex/ui/desktop";
import { ArrowRightIcon, HardDriveIcon, LockIcon, LockOpenIcon } from "@aidex/ui/icons";
import { Button, Card, Container, Heading, IconBadge, Stack, StatusIndicator, Text } from "@aidex/ui/primitives";
import { removableLabels } from "@aidex/client/dto/drive";
import { ErrorKind, toAidexError } from "@aidex/client/errors";
import { formatBytes } from "@aidex/client/format";
import { call } from "@aidex/client/rpc";
import { useAidexStatus } from "../../hooks/useAidexStatus";
import { useDriveReport } from "../../hooks/useCoreReports";
import { useRpc } from "../../hooks/useRpc";
import { CoreStatusNotice } from "../system/CoreStatusNotice";
import { driveSourceLabels } from "../system/labels";
import { UnlockForm, type UnlockMethod } from "./UnlockForm";
import { VaultSetupForm } from "./VaultSetupForm";

type Props = { onContinue: () => void };

/**
 * Drive and vault before the workspace: the live drive, AiDEX Core status, and the drive's vault
 * through `vault.status`, `vault.init`, and `vault.unlock`. Secrets go straight to the daemon over
 * the authenticated local IPC and are never kept in this page's state after submit.
 */
export function UnlockPage({ onContinue }: Props) {
  const { status, daemonRunning } = useAidexStatus();
  const drive = useDriveReport();
  const vault = useRpc("vault.status");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null);
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

  const run = async (job: () => Promise<unknown>, failure: (message: string, kind: string) => string) => {
    setBusy(true);
    setError(undefined);
    try {
      await job();
      vault.refetch();
    } catch (caught) {
      const aidexError = toAidexError(caught);
      setError(failure(aidexError.message, aidexError.kind));
    } finally {
      setBusy(false);
    }
  };

  const unlock = ({ method, secret }: { method: UnlockMethod; secret: string }) =>
    void run(
      () => call("vault.unlock", method === "password" ? { passphrase: secret } : { recovery_key: secret }),
      (message, kind) =>
        kind === ErrorKind.vaultLocked
          ? `That ${method === "password" ? "passphrase" : "recovery key"} did not unlock the vault.`
          : message,
    );

  const create = ({ passphrase, recovery }: { passphrase: string; recovery: boolean }) =>
    void run(async () => {
      const created = await call("vault.init", { passphrase, recovery });
      if (created.recovery_key) setRecoveryKey(created.recovery_key);
    }, (message) => message);

  const state = vault.data?.state;
  const notSupported = vault.error?.kind === ErrorKind.methodNotFound;

  const vaultBody = () => {
    if (recoveryKey) {
      return (
        <Stack gap={5}>
          <MediaItem
            media={<IconBadge tone="accent" icon={<LockOpenIcon />} />}
            title="Vault created · save your recovery key"
            titleLevel={2}
            description="This key unlocks the vault if you forget the passphrase. It is shown only now; AiDEX keeps no copy."
          />
          <CodeBlock title="Recovery key" code={recoveryKey} />
          <Button
            variant="tonal"
            size="lg"
            align="center"
            fullWidth
            trailing={<ArrowRightIcon />}
            onClick={() => {
              setRecoveryKey(null);
              onContinue();
            }}
          >
            I saved it, continue
          </Button>
        </Stack>
      );
    }
    if (!daemonRunning) {
      return (
        <MediaItem
          media={<IconBadge icon={<LockIcon />} />}
          title="Vault status unavailable"
          titleLevel={2}
          description="AiDEX Core is not running, so the vault on this drive cannot be checked or unlocked."
          trailing={<StatusIndicator tone="neutral" label="Unknown" />}
        />
      );
    }
    if (notSupported) {
      return (
        <MediaItem
          media={<IconBadge icon={<LockIcon />} />}
          title="Vault not available"
          titleLevel={2}
          description="This AiDEX Core has no vault service, so there is nothing to unlock and no password is asked for."
          trailing={<StatusIndicator tone="warning" label="Not set up" />}
        />
      );
    }
    if (vault.error) return <Notice tone="warning">Cannot read the vault status: {vault.error.message}</Notice>;
    if (!vault.data) return <Text tone="secondary">Checking the vault…</Text>;
    switch (state) {
      case "missing":
        return <VaultSetupForm onSubmit={create} busy={busy} error={error} />;
      case "locked":
        return <UnlockForm onSubmit={unlock} busy={busy} error={error} />;
      case "unlocked":
        return (
          <MediaItem
            media={<IconBadge tone="accent" icon={<LockOpenIcon />} />}
            title="Vault unlocked"
            titleLevel={2}
            description={`${vault.data.secrets ?? 0} secrets on this drive. Agents receive handles, never values.`}
            trailing={<StatusIndicator tone="success" label="Unlocked" />}
          />
        );
      default:
        return (
          <Notice tone="danger" role="alert">
            The vault on this drive cannot be used{vault.data.message ? `: ${vault.data.message}` : "."} Nothing was changed.
          </Notice>
        );
    }
  };

  const unlocked = state === "unlocked";

  return (
    <FocusLayout>
      <Container size="xs" verticallyCentered>
        <Stack gap={10}>
          <Stack gap={2} align="center">
            <Brand size="xl" layout="stacked" />
            <Heading ref={headingRef} tabIndex={-1} level={1} size="title" align="center">
              Unlock your drive
            </Heading>
            <Text variant="lead" tone="secondary" align="center">
              Your encrypted AI workspace
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
              <Stack gap={6}>
                {vaultBody()}
                {recoveryKey ? null : (
                  <Button
                    variant={unlocked ? "tonal" : "ghost"}
                    size="lg"
                    align="center"
                    fullWidth
                    trailing={<ArrowRightIcon />}
                    onClick={onContinue}
                  >
                    {unlocked || !daemonRunning || notSupported ? "Continue" : "Continue without unlocking"}
                  </Button>
                )}
              </Stack>
            </Card>
          </Stack>
        </Stack>
      </Container>
    </FocusLayout>
  );
}
