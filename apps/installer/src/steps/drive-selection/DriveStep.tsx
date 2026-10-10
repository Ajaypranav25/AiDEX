import { Notice, PageHeader, SectionHeader } from "@aidex/ui/components";
import { FolderIcon, RefreshIcon } from "@aidex/ui/icons";
import { Badge, ChoiceCardGroup, IconButton, Stack, Text, TextField, type ChoiceCardOption } from "@aidex/ui/primitives";
import { isEligibleVolume, removableLabels, type VolumeInfo } from "@aidex/client/dto/drive";
import { ErrorKind } from "@aidex/client/errors";
import { formatBytes } from "@aidex/client/format";
import { estimateGb, fitsOnDrive, formatGb } from "../../lib/setup";
import { stepKicker } from "../../lib/steps";
import type { DrivesQuery } from "../../lib/useDrives";
import type { StepProps } from "../types";

type Props = StepProps & { drives: DrivesQuery };

function volumeBadge(volume: VolumeInfo) {
  if (volume.system) return <Badge tone="neutral">System drive</Badge>;
  if (volume.has_aidex_manifest) {
    return <Badge tone="accent">{volume.schema_version ? `AiDEX drive · schema v${volume.schema_version}` : "Unreadable AiDEX manifest"}</Badge>;
  }
  if (!isEligibleVolume(volume)) return <Badge tone="neutral">{volume.removable === "fixed" ? "Internal disk" : "Not supported"}</Badge>;
  return <Badge tone="success">Ready</Badge>;
}

function option(volume: VolumeInfo): ChoiceCardOption<string> {
  const capacity =
    volume.total_bytes === null ? "capacity unknown" : `${formatBytes(volume.free_bytes)} free of ${formatBytes(volume.total_bytes)}`;
  return {
    value: volume.root,
    title: `${volume.label ?? "Untitled drive"} (${volume.root})`,
    description: [removableLabels[volume.removable], volume.filesystem, capacity].filter(Boolean).join(" · "),
    meta: volumeBadge(volume),
    disabled: !isEligibleVolume(volume),
  };
}

/** Step 1: choose a removable drive from the real volume list; the system drive and internal disks cannot be chosen. */
export function DriveStep({ state, update, titleRef, volume, drives }: Props) {
  const options = drives.volumes.map(option);
  const eligible = drives.volumes.filter(isEligibleVolume);

  return (
    <>
      <PageHeader
        eyebrow={stepKicker(0)}
        title="Choose a drive"
        description="Select the USB drive or portable SSD that will hold your portable AiDEX environment."
        titleRef={titleRef}
      />
      <Stack gap={3}>
        <SectionHeader
          title="Target drive"
          level={3}
          meta={drives.loading ? "Scanning…" : `${eligible.length} of ${drives.volumes.length} drives can be used`}
          actions={<IconButton label="Rescan drives" icon={<RefreshIcon />} variant="soft" size="lg" onClick={drives.rescan} />}
        />
        {drives.error ? (
          <Notice tone="warning">
            {drives.error.kind === ErrorKind.bridgeUnavailable
              ? "Drive discovery needs the AiDEX Setup app. This browser preview cannot read drives."
              : `Cannot list drives: ${drives.error.message}`}
          </Notice>
        ) : null}
        {options.length ? (
          <ChoiceCardGroup
            label="Target drive"
            labelVisibility="hidden"
            variant="rows"
            options={options}
            value={state.driveRoot ?? ""}
            onValueChange={(root) => update({ driveRoot: root })}
          />
        ) : drives.loading ? (
          <Text tone="secondary">Looking for drives…</Text>
        ) : drives.error ? null : (
          <Text tone="secondary">No drives were found. Connect a USB drive or portable SSD, then rescan.</Text>
        )}
        {!drives.loading && !drives.error && options.length > 0 && eligible.length === 0 ? (
          <Notice tone="warning">
            None of these drives can hold AiDEX. Connect a USB drive or portable SSD; the system drive and internal disks are never
            used.
          </Notice>
        ) : null}
      </Stack>
      <TextField
        label="Installation location"
        leadingIcon={<FolderIcon />}
        value={volume ? `${volume.root} (drive root)` : "Choose a drive"}
        readOnly
        size="md"
      />
      <Notice>
        AiDEX creates its own folders and an aidex-drive.json manifest at the drive root. Existing files stay untouched, and setup
        never formats, repartitions, or deletes anything.
      </Notice>
      {volume?.has_aidex_manifest ? (
        volume.schema_version === null ? (
          <Notice tone="danger" role="alert">
            {volume.root} has an aidex-drive.json that cannot be read. Setup will not change it; move it aside first.
          </Notice>
        ) : (
          <Notice tone="success">
            {volume.root} is already an AiDEX drive (schema v{volume.schema_version}). Setup keeps its contents and only adds what is
            missing{volume.schema_version < 2 ? ", migrating the manifest with a backup" : ""}.
          </Notice>
        )
      ) : null}
      {volume && !fitsOnDrive(state, volume) ? (
        <Notice tone="danger" role="alert">
          This drive has {formatBytes(volume.free_bytes)} free, but the selected setup needs about {formatGb(estimateGb(state))}.
          Choose another drive or a smaller setup.
        </Notice>
      ) : null}
    </>
  );
}
