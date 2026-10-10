import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { ChatIcon } from "@aidex/ui/icons";
import { Select } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { tabOf } from "../../lib/routes";
import { sampleSessions } from "../../lib/sample/sessions";
import { sampleWorkspaces, workspaceById } from "../../lib/sample/workspaces";
import { workspaceTabs } from "./workspaceTabs";

const RECENT_LIMIT = 4;
const workspaceOptions = sampleWorkspaces.map(({ id, name }) => ({ value: id, label: name }));

export function WorkspaceOverviewSidebar() {
  const { route, navigate, replace } = useNavigation();
  const { workspaceId, setWorkspaceId } = usePreferences();
  const workspace = workspaceById(route.id ?? workspaceId);
  const tab = tabOf(route, "workspace");
  const recentSessions = sampleSessions.filter((session) => session.workspaceId === workspace.id).slice(0, RECENT_LIMIT);

  const switchWorkspace = (id: string) => {
    setWorkspaceId(id);
    navigate({ page: "workspace", id, tab });
  };

  return (
    <SectionSidebar
      title="Workspace"
      headerExtra={
        <Select
          label="Current workspace"
          labelVisibility="hidden"
          options={workspaceOptions}
          value={workspace.id}
          onChange={(event) => switchWorkspace(event.target.value)}
        />
      }
    >
      <SidebarSection title="Project">
        {workspaceTabs.map(({ value, label, Icon }) => (
          <SidebarItem
            key={value}
            label={label}
            icon={<Icon />}
            active={value === tab}
            onSelect={() => replace({ ...route, id: workspace.id, tab: value })}
          />
        ))}
      </SidebarSection>
      <SidebarSection title="Recent sessions">
        {recentSessions.length > 0 ? (
          recentSessions.map((session) => (
            <SidebarItem
              key={session.id}
              label={session.title}
              size="sm"
              onSelect={() => navigate({ page: session.kind, id: session.id })}
            />
          ))
        ) : (
          <SidebarItem
            label={`Start a chat in ${workspace.name}`}
            icon={<ChatIcon />}
            onSelect={() => {
              setWorkspaceId(workspace.id);
              navigate({ page: "chat" });
            }}
          />
        )}
      </SidebarSection>
    </SectionSidebar>
  );
}
