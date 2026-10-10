import { useRef, type Ref } from "react";
import { EmptyState, KeyValueList, List, ListRow, SectionHeader } from "@aidex/ui/components";
import { SmartphoneIcon } from "@aidex/ui/icons";
import { Button, IconBadge, Stack } from "@aidex/ui/primitives";
import { PairingCard } from "./PairingCard";
import { sampleEncryption, type TrustedDevice } from "./sample";

type Props = {
  devices: readonly TrustedDevice[];
  onRevoke: (device: TrustedDevice) => void;
  pairing: boolean;
  pairingCardRef: Ref<HTMLDivElement>;
  onStartPairing: () => void;
  onCancelPairing: () => void;
};

/** Pairing, trusted devices, and how the link between phone and desktop is protected. */
export function DevicesPanel({ devices, onRevoke, pairing, pairingCardRef, onStartPairing, onCancelPairing }: Props) {
  // A revoked row unmounts, so focus moves to a neighbouring row's button, or to the empty state.
  const revokeRefs = useRef(new Map<string, HTMLButtonElement>());
  const emptyRef = useRef<HTMLDivElement>(null);
  const registerRevoke = (id: string) => (element: HTMLButtonElement | null) => {
    if (element) revokeRefs.current.set(id, element);
    else revokeRefs.current.delete(id);
  };

  const revoke = (device: TrustedDevice) => {
    const index = devices.findIndex((candidate) => candidate.id === device.id);
    const neighbourId = (devices[index + 1] ?? devices[index - 1])?.id;
    onRevoke(device);
    requestAnimationFrame(() => (neighbourId ? revokeRefs.current.get(neighbourId) : emptyRef.current)?.focus());
  };

  return (
    <Stack gap={10}>
      {pairing ? <PairingCard cardRef={pairingCardRef} onCancel={onCancelPairing} /> : null}
      <section>
        <SectionHeader title="Trusted devices" meta={`${devices.length} paired`} />
        {devices.length === 0 ? (
          <EmptyState
            ref={emptyRef}
            tabIndex={-1}
            icon={<SmartphoneIcon size={32} />}
            title="No devices paired"
            titleLevel={3}
            description="Pair a trusted device to review approvals and control running sessions remotely."
            actions={
              pairing ? null : (
                <Button variant="outline" size="md" onClick={onStartPairing}>
                  Pair new device
                </Button>
              )
            }
          />
        ) : (
          <List>
            {devices.map((device) => (
              <ListRow
                key={device.id}
                media={<IconBadge icon={<SmartphoneIcon />} size="md" />}
                title={device.name}
                subtitle={`${device.platform} · ${device.lastSeen}`}
                trailing={
                  <Button
                    ref={registerRevoke(device.id)}
                    variant="ghost"
                    size="md"
                    aria-label={`Revoke ${device.name}`}
                    onClick={() => revoke(device)}
                  >
                    Revoke
                  </Button>
                }
              />
            ))}
          </List>
        )}
      </section>
      <section>
        <SectionHeader title="Encryption" meta="Applies to every paired device" />
        <KeyValueList items={sampleEncryption} />
      </section>
    </Stack>
  );
}
