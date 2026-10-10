import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { BotIcon, ChatIcon, FolderIcon, HistoryIcon } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { sampleSessions } from "../../lib/sample/sessions";
import { sampleWorkspaces } from "../../lib/sample/workspaces";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { kindFilterOf, matchesKind, sessionsInWorkspace, workspaceFilterOf, type KindFilter } from "./historyFilters";

const kindItems: readonly { kind: KindFilter; label: string; Icon: typeof HistoryIcon }[] = [
  { kind: "all", label: "All sessions", Icon: HistoryIcon },
  { kind: "chat", label: "Chats", Icon: ChatIcon },
  { kind: "agent", label: "Agent runs", Icon: BotIcon },
];

/** History filters: session kind (route tab) and workspace (route id), shared with the page through the route. */
export function HistorySidebar() {
  const { route, replace } = useNavigation();
  const kind = kindFilterOf(route);
  const workspace = workspaceFilterOf(route);
  // Kind counts follow the workspace filter so they match what the page will list.
  const scoped = sessionsInWorkspace(workspace?.id);

  const showKind = (next: KindFilter) => replace({ page: "history", tab: next, id: workspace?.id });
  const showWorkspace = (id: string | undefined) => replace({ page: "history", tab: kind, id });

  return (
    <SectionSidebar title="History">
      <SidebarSection title="Filter">
        {kindItems.map((item) => (
          <SidebarItem
            key={item.kind}
            label={item.label}
            icon={<item.Icon />}
            count={scoped.filter((session) => matchesKind(session, item.kind)).length}
            active={kind === item.kind}
            onSelect={() => showKind(item.kind)}
          />
        ))}
      </SidebarSection>
      <SidebarSection title="Workspaces">
        <SidebarItem
          label="All workspaces"
          icon={<FolderIcon />}
          count={sampleSessions.length}
          active={!workspace}
          onSelect={() => showWorkspace(undefined)}
        />
        {sampleWorkspaces.map(({ id, name, Icon, iconTone }) => (
          <SidebarItem
            key={id}
            label={name}
            icon={<Icon />}
            iconTone={iconTone}
            count={sessionsInWorkspace(id).length}
            active={workspace?.id === id}
            onSelect={() => showWorkspace(id)}
          />
        ))}
      </SidebarSection>
    </SectionSidebar>
  );
}
