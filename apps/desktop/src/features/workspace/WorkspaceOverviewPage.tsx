import { PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { BotIcon, ChatIcon, GitBranchIcon, LaptopIcon, UsbDriveIcon } from "@aidex/ui/icons";
import { Badge, Button, Stack, TabPanel, Tabs, Text } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { tabOf } from "../../lib/routes";
import { sampleSessions } from "../../lib/sample/sessions";
import { workspaceById } from "../../lib/sample/workspaces";
import { EnvironmentPanel } from "./EnvironmentPanel";
import { FilesPanel } from "./FilesPanel";
import { GitPanel } from "./GitPanel";
import { MemoryPanel } from "./MemoryPanel";
import { OverviewPanel } from "./OverviewPanel";
import { locationLabel } from "./sample";
import { projectProfileFor } from "./sampleProjects";
import { SessionsPanel } from "./SessionsPanel";
import { workspaceTabs, type WorkspaceTab } from "./workspaceTabs";

const TABS_ID = "workspace-tabs";

export function WorkspaceOverviewPage() {
  const { route, navigate, replace } = useNavigation();
  const { workspaceId, setWorkspaceId } = usePreferences();
  const workspace = workspaceById(route.id ?? workspaceId);
  const profile = projectProfileFor(workspace.languages);
  const sessions = sampleSessions.filter((session) => session.workspaceId === workspace.id);
  const tab = tabOf(route, "workspace");
  const LocationIcon = workspace.location === "drive" ? UsbDriveIcon : LaptopIcon;

  const setTab = (next: WorkspaceTab) => replace({ ...route, id: workspace.id, tab: next });
  const openWith = (page: "chat" | "agent") => {
    setWorkspaceId(workspace.id);
    navigate({ page });
  };

  const counts: Partial<Record<WorkspaceTab, number>> = { git: profile.changes.length, sessions: sessions.length };
  const tabItems = workspaceTabs.map(({ value, label, Icon }) => ({ value, label, icon: <Icon />, count: counts[value] }));

  const meta = (
    <Stack direction="row" gap={4} align="center" wrap>
      <Stack direction="row" gap={1} align="center">
        <GitBranchIcon size={16} />
        <Text as="span" tone="secondary">
          {workspace.branch}
        </Text>
      </Stack>
      <Stack direction="row" gap={1} align="center">
        <LocationIcon size={16} />
        <Text as="span" tone="secondary">
          {locationLabel[workspace.location]}
        </Text>
      </Stack>
      <Stack direction="row" gap={2} align="center" wrap role="list" aria-label="Languages">
        {workspace.languages.map((language) => (
          <Badge key={language} tone="neutral" role="listitem">
            {language}
          </Badge>
        ))}
      </Stack>
    </Stack>
  );

  return (
    <PageFrame width="wide">
      <PageHeader
        title={workspace.name}
        description={workspace.path}
        meta={meta}
        back={{ label: "Workspaces", onClick: () => navigate({ page: "workspaces" }) }}
        actions={
          <>
            <Button variant="outline" size="md" icon={<ChatIcon />} onClick={() => openWith("chat")}>
              Ask about this project
            </Button>
            <Button variant="primary" size="md" icon={<BotIcon />} onClick={() => openWith("agent")}>
              Run agent
            </Button>
          </>
        }
      />
      <Tabs id={TABS_ID} label={`${workspace.name} sections`} variant="underline" items={tabItems} value={tab} onValueChange={setTab} />
      <TabPanel tabsId={TABS_ID} value={tab}>
        {tab === "overview" ? <OverviewPanel workspace={workspace} profile={profile} sessionCount={sessions.length} onTabChange={setTab} /> : null}
        {tab === "files" ? <FilesPanel workspace={workspace} profile={profile} /> : null}
        {tab === "git" ? <GitPanel workspace={workspace} profile={profile} /> : null}
        {tab === "environment" ? <EnvironmentPanel workspace={workspace} profile={profile} /> : null}
        {tab === "sessions" ? <SessionsPanel workspace={workspace} sessions={sessions} /> : null}
        {tab === "memory" ? <MemoryPanel workspace={workspace} profile={profile} /> : null}
      </TabPanel>
    </PageFrame>
  );
}
