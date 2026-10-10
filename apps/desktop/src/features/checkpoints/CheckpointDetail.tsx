import { useRef, useState } from "react";
import { KeyValueList, SectionHeader } from "@aidex/ui/components";
import { GitBranchIcon, GitCompareIcon, MoreHorizontalIcon, PlayIcon, RefreshIcon } from "@aidex/ui/icons";
import { Button, Divider, IconButton, Stack, Text } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { CheckpointComparison } from "./CheckpointComparison";
import { RestoreConfirmation } from "./RestoreConfirmation";
import type { SampleCheckpoint } from "./sample";

type Props = { checkpoint: SampleCheckpoint; workspaceName: string };

/**
 * Selected checkpoint: actions, optional comparison, and snapshot details.
 * Mount it with `key={checkpoint.id}` so compare and confirm state reset per checkpoint.
 */
export function CheckpointDetail({ checkpoint, workspaceName }: Props) {
  const notify = useToast();
  const [comparing, setComparing] = useState(false);
  const [confirmingRestore, setConfirmingRestore] = useState(false);
  const restoreButtonRef = useRef<HTMLButtonElement>(null);

  const closeConfirmation = () => {
    setConfirmingRestore(false);
    restoreButtonRef.current?.focus();
  };

  const confirmRestore = () => {
    notify("Preview: restoring a checkpoint will be available when AiDEX Core is connected.");
    closeConfirmation();
  };

  const changedFiles = checkpoint.changes.length;

  return (
    <section>
      <Stack gap={6}>
        <Stack gap={2}>
          <SectionHeader
            title={checkpoint.title}
            actions={
              <IconButton
                label="Checkpoint options"
                icon={<MoreHorizontalIcon />}
                onClick={() => notify("Preview: rename, export, and delete will be available when AiDEX Core is connected.")}
              />
            }
          />
          <Text tone="secondary">
            {checkpoint.time} · {checkpoint.description}
          </Text>
        </Stack>
        <Stack direction="row" gap={3} align="center" wrap role="group" aria-label="Checkpoint actions">
          <Button
            variant="primary"
            size="md"
            icon={<PlayIcon />}
            onClick={() => notify("Preview: resuming starts a new session from this checkpoint once AiDEX Core is connected.")}
          >
            Resume
          </Button>
          <Button
            ref={restoreButtonRef}
            variant="outline"
            size="md"
            icon={<RefreshIcon />}
            aria-expanded={confirmingRestore}
            onClick={() => setConfirmingRestore(true)}
          >
            Restore
          </Button>
          <Button
            variant="outline"
            size="md"
            icon={<GitBranchIcon />}
            onClick={() => notify("Preview: branching from a checkpoint will be available when AiDEX Core is connected.")}
          >
            Branch from here
          </Button>
          <Button
            variant={comparing ? "tonal" : "outline"}
            size="md"
            icon={<GitCompareIcon />}
            aria-pressed={comparing}
            onClick={() => setComparing((current) => !current)}
          >
            Compare
          </Button>
        </Stack>
        {confirmingRestore ? (
          <RestoreConfirmation
            checkpointTitle={checkpoint.title}
            time={checkpoint.time}
            onConfirm={confirmRestore}
            onCancel={closeConfirmation}
          />
        ) : null}
        {comparing ? <CheckpointComparison checkpoint={checkpoint} /> : null}
        <Divider />
        <section>
          <Stack gap={3}>
            <SectionHeader title="Snapshot details" level={3} />
            <KeyValueList
              items={[
                { label: "Changed files", value: String(changedFiles) },
                { label: "Workspace", value: workspaceName },
                { label: "Task state", value: checkpoint.taskState },
                { label: "Model", value: checkpoint.model },
              ]}
            />
            <Text tone="muted">Restoring changes the workspace. Resuming starts a new session from this point.</Text>
          </Stack>
        </section>
      </Stack>
    </section>
  );
}
