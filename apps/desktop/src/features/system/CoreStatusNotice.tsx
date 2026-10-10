import { Notice } from "@aidex/ui/components";
import { PlayIcon, RefreshIcon } from "@aidex/ui/icons";
import { Button, Stack, Text } from "@aidex/ui/primitives";
import { useAidexStatus } from "../../hooks/useAidexStatus";

type Props = {
  /** What the surrounding panel needs: a resolved drive, or a daemon that answers. */
  requires: "drive" | "daemon";
};

/**
 * Explains why live data is missing (outside the app, no drive, daemon not running,
 * incompatible daemon, error) and offers the fix. Renders nothing when the requirement is met.
 */
export function CoreStatusNotice({ requires }: Props) {
  const { status, error, loading, bridgeMissing, pending, actionError, startDaemon, refresh } = useAidexStatus();

  if (!status && loading) return <Notice>Checking AiDEX Core…</Notice>;
  if (bridgeMissing) {
    return (
      <Notice tone="warning">
        Live data is available only inside the AiDEX app. This browser preview cannot reach AiDEX Core.
      </Notice>
    );
  }
  if (error || !status) {
    return (
      <Notice tone="danger" role="alert">
        <Stack gap={3} align="start">
          <Text as="span">Cannot read the AiDEX status: {error?.message ?? "no answer"}</Text>
          <Button variant="outline" size="md" icon={<RefreshIcon />} onClick={() => void refresh()}>
            Retry
          </Button>
        </Stack>
      </Notice>
    );
  }

  const { drive, daemon } = status;
  if (drive.state === "none") {
    return (
      <Notice tone="warning">
        No AiDEX drive is selected for this session. Run AiDEX from its drive, set AIDEX_DRIVE_ROOT, or choose a drive on the
        launch screen.
      </Notice>
    );
  }
  if (drive.state === "invalid") {
    return (
      <Notice tone="danger">
        The configured drive {drive.path} cannot be used: {drive.error.message}
      </Notice>
    );
  }
  if (requires === "drive") return null;

  switch (daemon.state) {
    case "running":
    case "no_drive":
      return null;
    case "stopped":
      return (
        <Notice tone="warning">
          <Stack gap={3} align="start">
            <Text as="span">AiDEX Core is not running for {drive.root}.</Text>
            <Button
              variant="primary"
              size="md"
              icon={<PlayIcon />}
              disabled={pending !== null}
              onClick={() => void startDaemon()}
            >
              {pending === "start" ? "Starting AiDEX Core…" : "Start AiDEX Core"}
            </Button>
            {actionError ? (
              <Text as="span" tone="secondary" role="alert">
                Could not start AiDEX Core: {actionError.message}
              </Text>
            ) : null}
          </Stack>
        </Notice>
      );
    case "incompatible":
      return (
        <Notice tone="danger">
          The running AiDEX Core speaks a different protocol version than this app: {daemon.error.message}
        </Notice>
      );
    case "unauthorized":
      return (
        <Notice tone="danger">
          AiDEX Core rejected this app&apos;s credentials: {daemon.error.message}
        </Notice>
      );
    case "error":
      return (
        <Notice tone="danger">
          AiDEX Core did not answer as expected: {daemon.error.message}
        </Notice>
      );
  }
}
