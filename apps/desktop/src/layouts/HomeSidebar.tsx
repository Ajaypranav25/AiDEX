import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { MoreHorizontalIcon, PlusCircleIcon } from "@aidex/ui/icons";
import { IconButton, Kbd } from "@aidex/ui/primitives";
import { useNavigation } from "../app/providers/navigation";
import { usePreferences } from "../app/providers/preferences";
import { useToast } from "../app/providers/toast";
import { useModifierKeyLabel } from "../hooks/useModifierKeyLabel";
import { sampleSessions } from "../lib/sample/sessions";
import { sampleWorkspaces } from "../lib/sample/workspaces";
import { SectionSidebar } from "./SectionSidebar";
import { useShell } from "./shell";

const RECENT_LIMIT = 7;

export function HomeSidebar() {
  const { navigate } = useNavigation();
  const { workspaceId, setWorkspaceId } = usePreferences();
  const notify = useToast();
  const shell = useShell();
  const modifierKey = useModifierKeyLabel();

  const openWorkspace = (id: string) => {
    setWorkspaceId(id);
    navigate({ page: "workspace", id });
  };

  return (
    <SectionSidebar
      title="AiDEX"
      brandSize="lg"
      action={{
        label: "New session",
        icon: <PlusCircleIcon />,
        onClick: shell.startNewSession,
        trailing: <Kbd aria-hidden="true">{modifierKey} K</Kbd>,
        keyShortcuts: modifierKey === "⌘" ? "Meta+K" : "Control+K",
      }}
    >
      <SidebarSection title="Pinned (sample)">
        {sampleWorkspaces
          .filter((workspace) => workspace.pinned)
          .map(({ id, name, Icon, iconTone }) => (
            <SidebarItem
              key={id}
              label={name}
              icon={<Icon />}
              iconTone={iconTone}
              active={id === workspaceId}
              onSelect={() => openWorkspace(id)}
              actions={
                <IconButton
                  size="sm"
                  label={`More actions for ${name}`}
                  icon={<MoreHorizontalIcon />}
                  onClick={() => notify(`Workspace actions for ${name} will open here.`)}
                />
              }
            />
          ))}
      </SidebarSection>
      <SidebarSection title="Workspaces (sample)">
        {sampleWorkspaces
          .filter((workspace) => !workspace.pinned)
          .slice(0, 4)
          .map(({ id, name, Icon, iconTone }) => (
            <SidebarItem
              key={id}
              label={name}
              icon={<Icon />}
              iconTone={iconTone}
              active={id === workspaceId}
              onSelect={() => openWorkspace(id)}
            />
          ))}
      </SidebarSection>
      <SidebarSection title="Recents (sample)">
        {sampleSessions.slice(0, RECENT_LIMIT).map((session) => (
          <SidebarItem
            key={session.id}
            label={session.title}
            size="sm"
            onSelect={() => navigate({ page: session.kind === "agent" ? "agent" : "chat", id: session.id })}
          />
        ))}
      </SidebarSection>
    </SectionSidebar>
  );
}
