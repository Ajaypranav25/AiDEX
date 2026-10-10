import { DataTable, MediaItem, SectionHeader } from "@aidex/ui/components";
import { InfoIcon, PlayIcon, RefreshIcon, StethoscopeIcon } from "@aidex/ui/icons";
import { Button, Card, Stack, StatusIndicator } from "@aidex/ui/primitives";
import type { Notify } from "../../lib/useToasts";
import { integrityChecks } from "../installation/sample";

type Props = {
  driveName: string;
  onRunAgain: () => void;
  notify: Notify;
};

/** End of the preview run: says plainly that nothing was installed and lists the checks a real run performs. */
export function CompletionPanel({ driveName, onRunAgain, notify }: Props) {
  return (
    <section>
      <Stack gap={5}>
        <Card padding="md">
          <MediaItem
            media={<InfoIcon size={22} />}
            title="Preview run finished"
            titleLevel={3}
            description={`Nothing was written to ${driveName}, so the drive is not ready to use. A real installation verifies every file before it reports success.`}
          />
        </Card>
        <SectionHeader title="Integrity results" level={3} meta="Preview · not verified" />
        <DataTable
          caption="Integrity checks a real installation runs"
          hideCaption
          rowKey={(row) => row.id}
          rows={integrityChecks}
          columns={[
            { key: "check", header: "Check", rowHeader: true },
            { key: "scope", header: "What it verifies" },
            { key: "result", header: "Result", render: () => <StatusIndicator label="Not run in preview" tone="neutral" /> },
          ]}
        />
        <Stack direction="row" gap={3} wrap>
          <Button
            variant="primary"
            icon={<PlayIcon />}
            onClick={() => notify("Preview: AiDEX was not installed, so there is nothing to launch yet.")}
          >
            Launch AiDEX
          </Button>
          <Button
            icon={<StethoscopeIcon />}
            onClick={() => notify("Preview: repair will re-check and restore drive files once provisioning exists. Nothing was changed.")}
          >
            Repair
          </Button>
          <Button variant="ghost" icon={<RefreshIcon />} onClick={onRunAgain}>
            Run preview again
          </Button>
        </Stack>
      </Stack>
    </section>
  );
}
