import { useRef, useState } from "react";
import { PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { PlusIcon } from "@aidex/ui/icons";
import { Button, StatusIndicator, TabPanel, Tabs } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { useToast } from "../../app/providers/toast";
import { tabOf, type PageTab } from "../../lib/routes";
import { DevicesPanel } from "./DevicesPanel";
import { PAIRING_ROUTE_ID, sampleRemoteSessions, sampleTrustedDevices, type RemoteSession, type TrustedDevice } from "./sample";
import { SessionsPanel, type EndAction } from "./SessionsPanel";

type RemoteTab = PageTab<"remote">;

const TABS_ID = "remote-tabs";

export function RemotePage() {
  const { route, replace } = useNavigation();
  const notify = useToast();
  const tab = tabOf(route, "remote");
  // Pairing lives in the route so the sidebar's "Pair device" can open it too.
  const pairing = tab === "devices" && route.id === PAIRING_ROUTE_ID;
  // Held here, not in the panels, so revoked devices and ended sessions stay gone across tab switches.
  const [devices, setDevices] = useState<readonly TrustedDevice[]>(sampleTrustedDevices);
  const [sessions, setSessions] = useState<readonly RemoteSession[]>(sampleRemoteSessions);
  const pairButtonRef = useRef<HTMLButtonElement>(null);
  const pairingCardRef = useRef<HTMLDivElement>(null);

  const tabItems: readonly { value: RemoteTab; label: string; count?: number }[] = [
    { value: "devices", label: "Devices" },
    { value: "sessions", label: "Sessions", count: sessions.length },
  ];

  const selectTab = (next: RemoteTab) => replace({ page: "remote", tab: next });
  const startPairing = () => {
    replace({ page: "remote", tab: "devices", id: PAIRING_ROUTE_ID });
    // The pressed button disables or unmounts, so focus lands on the pairing card it opened.
    requestAnimationFrame(() => pairingCardRef.current?.focus());
  };
  const cancelPairing = () => {
    replace({ page: "remote", tab: "devices" });
    requestAnimationFrame(() => pairButtonRef.current?.focus());
  };

  const revokeDevice = (device: TrustedDevice) => {
    setDevices((current) => current.filter((candidate) => candidate.id !== device.id));
    notify(`${device.name} removed from this preview. Revoking its keys will happen in AiDEX Core once connected.`);
  };

  const endSession = (session: RemoteSession, action: EndAction) => {
    setSessions((current) => current.filter((candidate) => candidate.id !== session.id));
    notify(
      action === "disconnect"
        ? `Preview: ${session.device} disconnected here; ending the live link needs AiDEX Core.`
        : `Preview: ${session.device}'s access revoked here; Core will invalidate its keys once connected.`,
    );
  };

  return (
    <PageFrame width="default">
      <PageHeader
        title="Remote access"
        description="Pair a trusted phone to review approvals and control running sessions."
        actions={
          <Button ref={pairButtonRef} variant="primary" size="md" icon={<PlusIcon />} onClick={startPairing} disabled={pairing}>
            Pair device
          </Button>
        }
        meta={<StatusIndicator tone="neutral" label="Sample data until AiDEX Core is connected" />}
      />
      <Tabs
        id={TABS_ID}
        label="Remote access sections"
        variant="underline"
        items={tabItems}
        value={tab}
        onValueChange={selectTab}
      />
      <TabPanel tabsId={TABS_ID} value={tab}>
        {tab === "devices" ? (
          <DevicesPanel
            devices={devices}
            onRevoke={revokeDevice}
            pairing={pairing}
            pairingCardRef={pairingCardRef}
            onStartPairing={startPairing}
            onCancelPairing={cancelPairing}
          />
        ) : (
          <SessionsPanel sessions={sessions} onEnd={endSession} />
        )}
      </TabPanel>
    </PageFrame>
  );
}
