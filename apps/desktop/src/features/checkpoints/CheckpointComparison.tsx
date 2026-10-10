import { DiffView, EmptyState, KeyValueList, SectionHeader } from "@aidex/ui/components";
import { CheckCircleIcon } from "@aidex/ui/icons";
import { Stack } from "@aidex/ui/primitives";
import type { SampleCheckpoint } from "./sample";

type Props = { checkpoint: SampleCheckpoint };

const fileCount = (count: number) => `${count} ${count === 1 ? "file" : "files"} differ`;

/** Differences between a checkpoint and the current workspace: one representative diff plus every changed file. */
export function CheckpointComparison({ checkpoint }: Props) {
  const { changes, diff } = checkpoint;
  return (
    <section>
      <Stack gap={4}>
        <SectionHeader title="Compared with current workspace" level={3} meta={fileCount(changes.length)} />
        {changes.length === 0 ? (
          <EmptyState
            icon={<CheckCircleIcon size={28} />}
            title="No differences"
            description="The current workspace matches this checkpoint."
          />
        ) : (
          <>
            <DiffView fileName={diff.fileName} additions={diff.additions} deletions={diff.deletions} lines={diff.lines} />
            <KeyValueList items={changes.map((change) => ({ label: change.path, value: change.summary }))} />
          </>
        )}
      </Stack>
    </section>
  );
}
