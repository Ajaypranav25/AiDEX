import type { Ref } from "react";
import { QrCodePattern, SectionHeader, StepList } from "@aidex/ui/components";
import { Button, Card, Split, Stack, StatusIndicator, Text } from "@aidex/ui/primitives";
import { samplePairing } from "./sample";

type Props = {
  onCancel: () => void;
  /** Focus target when pairing starts; the card is focusable but not in the tab order. */
  cardRef?: Ref<HTMLDivElement>;
};

const QR_SIZE = 168;

/** An open pairing session: the code to scan, the short code to confirm, and the steps on the phone. */
export function PairingCard({ onCancel, cardRef }: Props) {
  return (
    <section aria-label="Pair a new device">
      <Card ref={cardRef} tabIndex={-1}>
        <Stack gap={6}>
          <SectionHeader
            title="Pair a new device"
            actions={<StatusIndicator tone="warning" label={`Expires in ${samplePairing.expiresIn}`} />}
          />
          <Split emphasis="end" divided>
            <Stack gap={4} align="center">
              <QrCodePattern seed={samplePairing.code} size={QR_SIZE} />
              <Stack gap={1} align="center">
                <Text variant="label" tone="secondary" align="center">
                  Short code
                </Text>
                <Text variant="lead" align="center">
                  {samplePairing.code}
                </Text>
              </Stack>
            </Stack>
            <Stack gap={5}>
              <SectionHeader title="On your phone" level={3} />
              <StepList
                steps={samplePairing.steps.map((step, index) => ({ ...step, status: index === 0 ? "active" : "pending" }))}
              />
              <Text tone="muted">Preview: this code is a visual stand-in and cannot be scanned until AiDEX Core is connected.</Text>
              <Stack direction="row">
                <Button variant="outline" size="md" onClick={onCancel}>
                  Cancel pairing
                </Button>
              </Stack>
            </Stack>
          </Split>
        </Stack>
      </Card>
    </section>
  );
}
