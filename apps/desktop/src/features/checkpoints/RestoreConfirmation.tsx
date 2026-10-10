import { Button, Card, Heading, Stack, Text } from "@aidex/ui/primitives";

type Props = {
  checkpointTitle: string;
  time: string;
  onConfirm: () => void;
  onCancel: () => void;
};

/** Inline confirmation shown before a restore, since restoring replaces the current workspace state. */
export function RestoreConfirmation({ checkpointTitle, time, onConfirm, onCancel }: Props) {
  return (
    <Card padding="md" role="group" aria-labelledby="restore-confirmation-title">
      <Stack gap={4}>
        <Stack gap={2}>
          <Heading id="restore-confirmation-title" level={3}>
            Restore “{checkpointTitle}”?
          </Heading>
          <Text tone="secondary">
            The workspace returns to its state from {time}. Changes made since then are replaced. Save a checkpoint
            first if you want to keep them.
          </Text>
        </Stack>
        <Stack direction="row" gap={3} wrap>
          <Button variant="primary" size="md" onClick={onConfirm}>
            Confirm restore
          </Button>
          {/* Cancel takes focus: the safe choice is the default. */}
          <Button variant="ghost" size="md" onClick={onCancel} autoFocus>
            Cancel
          </Button>
        </Stack>
      </Stack>
    </Card>
  );
}
