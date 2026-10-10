import { EntityCard, MediaItem } from "@aidex/ui/components";
import { DownloadIcon, PuzzleIcon, ShieldIcon } from "@aidex/ui/icons";
import { Badge, Button, Card, Grid, IconBadge, Stack, Switch, TabPanel, Tabs, Text } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { installedPlugins, marketplacePlugins, type Plugin } from "./sample";
import type { Toggles } from "./useToggles";

export type PluginView = "installed" | "marketplace";

const VIEW_TABS_ID = "plugin-views";

const viewItems = [
  { value: "installed", label: "Installed", count: installedPlugins.length },
  { value: "marketplace", label: "Marketplace", count: marketplacePlugins.length },
] as const;

type Props = {
  view: PluginView;
  onViewChange: (view: PluginView) => void;
  toggles: Toggles;
  /** Installed plugin to mark as current, e.g. when opened from the sidebar. */
  focusId?: string;
};

function PermissionBadges({ plugin }: { plugin: Plugin }) {
  return (
    <>
      {plugin.permissions.map((permission) => (
        <Badge key={permission} tone={permission === "Shell" || permission === "Network" ? "accent" : "neutral"}>
          {permission}
        </Badge>
      ))}
    </>
  );
}

function InstalledPlugins({ toggles, focusId }: Pick<Props, "toggles" | "focusId">) {
  return (
    <Grid columns={2} gap={4}>
      {installedPlugins.map((plugin) => {
        const on = toggles.isOn(plugin.id);
        return (
          <EntityCard
            key={plugin.id}
            titleLevel={2}
            media={<IconBadge size="md" icon={<PuzzleIcon />} />}
            title={plugin.name}
            subtitle={`v${plugin.version} · ${plugin.publisher}`}
            badges={<PermissionBadges plugin={plugin} />}
            description={plugin.description}
            selected={plugin.id === focusId}
            footer={
              <Stack direction="row" gap={3} align="center">
                <Switch label={`Enable ${plugin.name}`} checked={on} onCheckedChange={(next) => toggles.set(plugin.id, next)} />
                <Text as="span" variant="label" tone="secondary" aria-hidden="true">
                  {on ? "Enabled" : "Disabled"}
                </Text>
              </Stack>
            }
          />
        );
      })}
    </Grid>
  );
}

function Marketplace() {
  const notify = useToast();
  return (
    <Stack gap={6}>
      <Card padding="md">
        <MediaItem
          media={<IconBadge tone="accent" icon={<ShieldIcon />} />}
          title="Third-party plugins run in the sandbox"
          description="Each plugin gets only the permissions on its card. Shell and network access ask for your approval the first time they are used."
        />
      </Card>
      <Grid columns={3} gap={4}>
        {marketplacePlugins.map((plugin) => (
          <EntityCard
            key={plugin.id}
            titleLevel={2}
            media={<IconBadge size="md" icon={<PuzzleIcon />} />}
            title={plugin.name}
            subtitle={`${plugin.publisher} · v${plugin.version}`}
            badges={<PermissionBadges plugin={plugin} />}
            description={plugin.description}
            footer={
              <Stack direction="row" gap={3} align="center" justify="between" wrap>
                <Text as="span" variant="label" tone="muted">
                  {plugin.permissions.length === 1 ? "1 permission" : `${plugin.permissions.length} permissions`}
                </Text>
                <Button
                  variant="outline"
                  icon={<DownloadIcon />}
                  aria-label={`Install ${plugin.name}`}
                  onClick={() => notify(`Preview: installing ${plugin.name} will be available when AiDEX Core is connected.`)}
                >
                  Install
                </Button>
              </Stack>
            }
          />
        ))}
      </Grid>
    </Stack>
  );
}

/** Plugins tab: installed plugins with permissions and enable switches, and the marketplace. */
export function PluginsPanel({ view, onViewChange, toggles, focusId }: Props) {
  return (
    <Stack gap={6}>
      <Tabs id={VIEW_TABS_ID} label="Plugin view" variant="pill" items={viewItems} value={view} onValueChange={onViewChange} />
      <TabPanel tabsId={VIEW_TABS_ID} value={view}>
        {view === "installed" ? <InstalledPlugins toggles={toggles} focusId={focusId} /> : <Marketplace />}
      </TabPanel>
    </Stack>
  );
}
