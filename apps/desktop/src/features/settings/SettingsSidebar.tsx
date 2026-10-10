import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { useNavigation } from "../../app/providers/navigation";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { tabOf } from "../../lib/routes";
import { settingsTabs } from "./tabs";

export function SettingsSidebar() {
  const { route, replace } = useNavigation();
  const tab = tabOf(route, "settings");

  return (
    <SectionSidebar title="Settings">
      <SidebarSection title="Preferences">
        {settingsTabs.map(({ value, label, Icon }) => (
          <SidebarItem
            key={value}
            label={label}
            icon={<Icon />}
            active={tab === value}
            onSelect={() => replace({ page: "settings", tab: value })}
          />
        ))}
      </SidebarSection>
    </SectionSidebar>
  );
}
