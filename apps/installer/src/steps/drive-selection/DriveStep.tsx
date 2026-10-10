import { useEffect, useState } from "react";
import { Meter, SectionHeader } from "@aidex/ui/components";
import { HardDriveIcon, RefreshIcon, UsbDriveIcon } from "@aidex/ui/icons";
import { Badge, Button, ChoiceCardGroup, Stack, Text, type ChoiceCardOption } from "@aidex/ui/primitives";
import type { Notify } from "../../lib/useToasts";
import { driveById, formatGb, sampleDrives, type Drive } from "../../lib/setup";

type Props = {
  driveId: string | null;
  onDriveChange: (id: string) => void;
  notify: Notify;
};

const RESCAN_MS = 900;
const RECOMMENDED_DRIVE = "samsung-t7";

function capacityText(drive: Drive): string {
  const capacity = drive.capacityGb >= 1000 ? `${drive.capacityGb / 1000} TB` : `${drive.capacityGb} GB`;
  const base = `${capacity} · ${drive.freeGb} GB free · ${drive.connection}`;
  return drive.unavailableReason ? `${base}. ${drive.unavailableReason}` : base;
}

function driveOption(drive: Drive): ChoiceCardOption<string> {
  const meta = drive.unavailableReason ? (
    <Badge tone="neutral">Too small</Badge>
  ) : drive.id === RECOMMENDED_DRIVE ? (
    <Badge tone="accent">Recommended</Badge>
  ) : undefined;
  return {
    value: drive.id,
    title: drive.name,
    description: capacityText(drive),
    icon: drive.capacityGb >= 500 ? <HardDriveIcon /> : <UsbDriveIcon />,
    meta,
    disabled: Boolean(drive.unavailableReason),
  };
}

/** Screen 29, drive half: sample removable drives with capacity, a fake rescan, and the chosen drive's usage. */
export function DriveStep({ driveId, onDriveChange, notify }: Props) {
  const [scanning, setScanning] = useState(false);
  const drive = driveById(driveId);
  const rescan = () => {
    // aria-disabled (not disabled) keeps keyboard focus on the button while the rescan runs.
    if (!scanning) setScanning(true);
  };

  useEffect(() => {
    if (!scanning) return;
    const timer = setTimeout(() => {
      setScanning(false);
      notify("Preview: drive list reloaded from sample data. No hardware was scanned.");
    }, RESCAN_MS);
    return () => clearTimeout(timer);
  }, [scanning, notify]);

  return (
    <section>
      <Stack gap={5}>
        <SectionHeader
          title="Select a drive"
          meta={scanning ? "Rescanning…" : `${sampleDrives.length} drives · sample list`}
          actions={
            <Button size="md" icon={<RefreshIcon />} aria-disabled={scanning} onClick={rescan}>
              {scanning ? "Rescanning…" : "Rescan drives"}
            </Button>
          }
        />
        <ChoiceCardGroup
          label="Detected drives"
          labelVisibility="hidden"
          options={sampleDrives.map(driveOption)}
          value={driveId ?? ""}
          onValueChange={onDriveChange}
        />
        {drive ? (
          <Meter
            label={`${drive.name} storage in use`}
            value={drive.capacityGb - drive.freeGb}
            max={drive.capacityGb}
            valueText={`${formatGb(drive.capacityGb - drive.freeGb)} used · ${formatGb(drive.freeGb)} free`}
          />
        ) : (
          <Text tone="muted">Choose a drive with at least 128 GB. You can change it until the final step.</Text>
        )}
        <Text tone="muted">
          These drives are sample data. This preview never scans, formats, or writes to a drive; the real installer will list every change before
          it writes anything.
        </Text>
      </Stack>
    </section>
  );
}
