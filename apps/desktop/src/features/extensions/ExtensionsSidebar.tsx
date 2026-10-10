import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import type { ComponentType } from "react";
import { LayersIcon, PuzzleIcon, ServerIcon, type IconProps } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { tabOf, type PageTab } from "../../lib/routes";
import { extensionShortcuts, installedPlugins, mcpServers, sampleSkills } from "./sample";

type ExtensionsTab = PageTab<"extensions">;

const browseItems = [
  { tab: "skills", label: "Skills", Icon: LayersIcon, count: sampleSkills.length },
  { tab: "mcp", label: "MCP servers", Icon: ServerIcon, count: mcpServers.length },
  { tab: "plugins", label: "Plugins", Icon: PuzzleIcon, count: installedPlugins.length },
] as const satisfies readonly { tab: ExtensionsTab; label: string; Icon: ComponentType<IconProps>; count: number }[];

/** Shortcut items reuse their tab's icon so they don't imply an enabled state the sidebar can't know. */
function TabIcon({ tab }: { tab: ExtensionsTab }) {
  const item = browseItems.find((entry) => entry.tab === tab);
  return item ? <item.Icon /> : null;
}

export function ExtensionsSidebar() {
  const { route, replace } = useNavigation();
  const tab = tabOf(route, "extensions");

  return (
    <SectionSidebar title="Plugins">
      <SidebarSection title="Browse">
        {browseItems.map(({ tab: itemTab, label, Icon, count }) => (
          <SidebarItem
            key={itemTab}
            label={label}
            icon={<Icon />}
            count={count}
            active={tab === itemTab}
            onSelect={() => replace({ page: "extensions", tab: itemTab })}
          />
        ))}
      </SidebarSection>
      <SidebarSection title="Shortcuts">
        {extensionShortcuts.map((item) => (
          <SidebarItem
            key={item.id}
            label={item.label}
            icon={<TabIcon tab={item.tab} />}
            active={tab === item.tab && route.id === item.id}
            onSelect={() => replace({ page: "extensions", tab: item.tab, id: item.id })}
          />
        ))}
      </SidebarSection>
    </SectionSidebar>
  );
}
