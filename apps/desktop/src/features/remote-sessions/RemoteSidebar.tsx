import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { PlusIcon, SmartphoneIcon, WifiIcon } from "@aidex/ui/icons";
import { Text } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { tabOf } from "../../lib/routes";
import { PAIRING_ROUTE_ID } from "./sample";

export function RemoteSidebar() {
  const { route, replace } = useNavigation();
  const tab = tabOf(route, "remote");
  const pairing = tab === "devices" && route.id === PAIRING_ROUTE_ID;

  return (
    <SectionSidebar title="Remote access">
      <SidebarSection title="Devices">
        <SidebarItem
          label="Paired devices"
          icon={<SmartphoneIcon />}
          active={tab === "devices" && !pairing}
          onSelect={() => replace({ page: "remote", tab: "devices" })}
        />
        <SidebarItem
          label="Pair device"
          icon={<PlusIcon />}
          active={pairing}
          onSelect={() => replace({ page: "remote", tab: "devices", id: PAIRING_ROUTE_ID })}
        />
      </SidebarSection>
      <SidebarSection title="Sessions">
        <SidebarItem
          label="Active sessions"
          icon={<WifiIcon />}
          active={tab === "sessions"}
          onSelect={() => replace({ page: "remote", tab: "sessions" })}
        />
      </SidebarSection>
      <Text tone="muted">Remote control is separate from the main desktop workflow.</Text>
    </SectionSidebar>
  );
}
