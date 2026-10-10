import { List, ListRow, Notice, PageHeader, SectionHeader } from "@aidex/ui/components";
import { FolderIcon, HardDriveIcon, RefreshIcon } from "@aidex/ui/icons";
import { IconButton, Select, Stack, Text, TextField } from "@aidex/ui/primitives";
import { driveById, drives, estimateGb, fitsOnDrive, formatGb, installPath, type DriveId } from "../../lib/setup";
import { stepKicker } from "../../lib/steps";
import type { StepProps } from "../types";

const driveOptions = drives.map((drive) => ({ value: drive.id, label: `${drive.name} (${drive.letter})` }));

/** Step 1: choose the target drive and see where AiDEX will be installed. */
export function DriveStep({ state, update, notify, titleRef }: StepProps) {
  const drive = driveById(state.driveId);

  return (
    <>
      <PageHeader
        eyebrow={stepKicker(0)}
        title="Choose a drive"
        description="Select the external USB or SSD that will hold your portable AiDEX environment."
        titleRef={titleRef}
      />
      <Stack gap={3}>
        <SectionHeader title="Target drive" level={3} />
        <Stack direction="row" gap={2} align="center">
          <Select
            label="Target drive"
            labelVisibility="hidden"
            options={driveOptions}
            value={state.driveId}
            onChange={(event) => update({ driveId: event.target.value as DriveId })}
          />
          <IconButton
            label="Rescan drives"
            icon={<RefreshIcon />}
            variant="soft"
            size="lg"
            onClick={() => notify("Sample devices only. No hardware scan occurred.")}
          />
        </Stack>
        <List>
          <ListRow
            media={<HardDriveIcon />}
            title="Available space"
            subtitle={`${drive.kind} · ${drive.freeGb} GB free of ${drive.totalGb} GB`}
            trailing={<Text as="span" tone="muted">{drive.letter}</Text>}
          />
        </List>
      </Stack>
      <TextField label="Installation folder" leadingIcon={<FolderIcon />} value={installPath(state)} readOnly size="md" />
      <Notice>Your existing files stay untouched. AiDEX installs into its own folder, and this preview never formats a drive.</Notice>
      {fitsOnDrive(state) ? null : (
        <Notice tone="danger" role="alert">
          This drive has {drive.freeGb} GB free, but the selected setup needs about {formatGb(estimateGb(state))}. Choose
          another drive or a smaller setup.
        </Notice>
      )}
    </>
  );
}
