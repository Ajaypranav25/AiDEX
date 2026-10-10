import { Meter } from "@aidex/ui/components";
import { Card, Stack, Text } from "@aidex/ui/primitives";
import { driveById, estimateGb, formatGb, type SetupSelections } from "../lib/setup";

type Props = { selections: SetupSelections };

/** Live estimate of the selected install size against the chosen drive's free space. */
export function SpaceSummary({ selections }: Props) {
  const drive = driveById(selections.driveId);
  const estimate = estimateGb(selections.components);
  if (!drive) {
    return (
      <Card padding="md">
        <Text tone="muted">Estimated size: {formatGb(estimate)}. Choose a drive to compare it with free space.</Text>
      </Card>
    );
  }
  const remaining = drive.freeGb - estimate;
  return (
    <Card padding="md">
      <Stack gap={3}>
        <Meter
          label={`Estimated size on ${drive.name}`}
          value={estimate}
          max={drive.freeGb}
          valueText={`${formatGb(estimate)} of ${formatGb(drive.freeGb)} free`}
        />
        <Text tone="muted">
          {remaining >= 0
            ? `${formatGb(remaining)} would remain for workspaces, sessions, and the vault.`
            : `This selection is ${formatGb(-remaining)} larger than the free space. Remove components or choose another drive.`}
        </Text>
      </Stack>
    </Card>
  );
}
