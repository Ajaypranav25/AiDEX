import { List, ListRow, MediaItem, SectionHeader, SegmentedMeter } from "@aidex/ui/components";
import { EjectIcon, HardDriveIcon, ShieldCheckIcon } from "@aidex/ui/icons";
import { Button, Card, IconBadge, Stack, StatusIndicator } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { ResidueCleanup } from "./ResidueCleanup";
import { sampleDrive, sampleIntegrity, sampleStorage } from "./sample";

const usedGb = sampleStorage.reduce((sum, segment) => sum + segment.value, 0);

/** The AiDEX drive: identity and health, storage breakdown, integrity, host cleanup, and safe eject. */
export function DrivePanel() {
  const notify = useToast();

  return (
    <Stack gap={10}>
      <Card padding="md">
        <MediaItem
          media={<IconBadge icon={<HardDriveIcon />} tone="accent" />}
          title={sampleDrive.name}
          titleLevel={2}
          description={sampleDrive.description}
          trailing={
            <Stack direction="row" gap={4} align="center" wrap>
              <StatusIndicator tone="success" label="Healthy" />
              <Button
                variant="primary"
                size="md"
                icon={<EjectIcon />}
                onClick={() =>
                  notify("Preview: safe eject will checkpoint running sessions first, then unmount the drive once AiDEX Core is connected.")
                }
              >
                Safe eject
              </Button>
            </Stack>
          }
        />
      </Card>
      <section>
        <SectionHeader title="Storage" />
        <SegmentedMeter
          label="Drive storage"
          segments={sampleStorage}
          max={sampleDrive.capacityGb}
          valueText={`${usedGb} GB of 1 TB used`}
          remainderLabel="Free"
          formatValue={(value) => `${value} GB`}
        />
      </section>
      <section>
        <SectionHeader
          title="Integrity"
          actions={
            <Button
              variant="outline"
              size="md"
              icon={<ShieldCheckIcon />}
              onClick={() => notify("Preview: verifying the drive will be available when AiDEX Core is connected.")}
            >
              Verify now
            </Button>
          }
        />
        <List>
          {sampleIntegrity.map((item) => (
            <ListRow
              key={item.id}
              title={item.title}
              subtitle={item.detail}
              trailing={item.id === "last-verified" ? null : <StatusIndicator tone="success" label="Passed" />}
            />
          ))}
        </List>
      </section>
      <ResidueCleanup />
    </Stack>
  );
}
