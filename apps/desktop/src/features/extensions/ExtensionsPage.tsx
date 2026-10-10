import { useEffect, useState } from "react";
import { PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { GridIcon, PlusIcon } from "@aidex/ui/icons";
import { Button, TabPanel, Tabs } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { useToast } from "../../app/providers/toast";
import { tabOf, type PageTab } from "../../lib/routes";
import { McpPanel } from "./McpPanel";
import { PluginsPanel, type PluginView } from "./PluginsPanel";
import { SkillsPanel } from "./SkillsPanel";
import { installedPlugins, mcpServers, sampleSkills } from "./sample";
import { useToggles } from "./useToggles";

type ExtensionsTab = PageTab<"extensions">;

const TABS_ID = "extensions-tabs";

const tabItems = [
  { value: "skills", label: "Skills", count: sampleSkills.length },
  { value: "mcp", label: "MCP servers", count: mcpServers.length },
  { value: "plugins", label: "Plugins", count: installedPlugins.length },
] as const satisfies readonly { value: ExtensionsTab; label: string; count: number }[];

export function ExtensionsPage() {
  const { route, replace } = useNavigation();
  const notify = useToast();
  const tab = tabOf(route, "extensions");
  // Toggles and the plugin view live here so they survive switching tabs.
  const skillToggles = useToggles(sampleSkills);
  const pluginToggles = useToggles(installedPlugins);
  const [pluginView, setPluginView] = useState<PluginView>("installed");

  // A sidebar shortcut to an installed plugin shows the installed view, where its card is marked.
  useEffect(() => {
    if (tab === "plugins" && route.id) setPluginView("installed");
  }, [tab, route.id]);

  const primary: Record<ExtensionsTab, { label: string; onClick: () => void }> = {
    skills: {
      label: "Install skill",
      onClick: () => notify("Preview: installing skills will be available when AiDEX Core is connected."),
    },
    mcp: {
      label: "Add server",
      onClick: () => notify("Preview: adding MCP servers will be available when AiDEX Core is connected."),
    },
    plugins: { label: "Browse marketplace", onClick: () => setPluginView("marketplace") },
  };
  const action = primary[tab];

  return (
    <PageFrame width="default">
      <PageHeader
        title="Plugins"
        description="Reusable skills, MCP servers, and extensions in one place."
        actions={
          <Button variant="primary" icon={tab === "plugins" ? <GridIcon /> : <PlusIcon />} onClick={action.onClick}>
            {action.label}
          </Button>
        }
      />
      <Tabs
        id={TABS_ID}
        label="Plugin sections"
        variant="underline"
        items={tabItems}
        value={tab}
        onValueChange={(next) => replace({ page: "extensions", tab: next })}
      />
      {/* Panels are keyed by the route id so a sidebar shortcut re-opens them with that item selected. */}
      <TabPanel tabsId={TABS_ID} value={tab}>
        {tab === "skills" ? <SkillsPanel key={route.id} toggles={skillToggles} initialEditingId={route.id} /> : null}
        {tab === "mcp" ? <McpPanel key={route.id} initialServerId={route.id} /> : null}
        {tab === "plugins" ? (
          <PluginsPanel view={pluginView} onViewChange={setPluginView} toggles={pluginToggles} focusId={route.id} />
        ) : null}
      </TabPanel>
    </PageFrame>
  );
}
