import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { BotIcon, ChatIcon, PencilIcon } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { useShell } from "../../layouts/shell";
import { sampleSessions, type SampleSession } from "../../lib/sample/sessions";
import { sampleWorkspaces } from "../../lib/sample/workspaces";

const WORKSPACE_LIMIT = 4;

export function ChatSidebar() {
  const { route, navigate } = useNavigation();
  const { workspaceId, setWorkspaceId } = usePreferences();
  const shell = useShell();

  const openSession = (session: SampleSession) =>
    navigate({ page: session.kind === "agent" ? "agent" : "chat", id: session.id });

  return (
    <SectionSidebar title="Chat" action={{ label: "New chat", icon: <PencilIcon />, onClick: shell.startNewSession }}>
      <SidebarSection title="Pinned">
        {sampleSessions
          .filter((session) => session.pinned)
          .map((session) => (
            <SidebarItem
              key={session.id}
              label={session.title}
              icon={session.kind === "agent" ? <BotIcon /> : <ChatIcon />}
              active={session.id === route.id}
              onSelect={() => openSession(session)}
            />
          ))}
      </SidebarSection>
      <SidebarSection title="Recent">
        {sampleSessions
          .filter((session) => session.kind === "chat" && !session.pinned)
          .map((session) => (
            <SidebarItem
              key={session.id}
              label={session.title}
              size="sm"
              active={session.id === route.id}
              onSelect={() => openSession(session)}
            />
          ))}
      </SidebarSection>
      {/* Choosing a workspace changes the chat's context; it does not leave the conversation. */}
      <SidebarSection title="Workspaces">
        {sampleWorkspaces.slice(0, WORKSPACE_LIMIT).map(({ id, name, Icon, iconTone }) => (
          <SidebarItem
            key={id}
            label={name}
            icon={<Icon />}
            iconTone={iconTone}
            active={id === workspaceId}
            onSelect={() => setWorkspaceId(id)}
          />
        ))}
      </SidebarSection>
    </SectionSidebar>
  );
}
