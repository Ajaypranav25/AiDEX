import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { BotIcon, GitCompareIcon, HistoryIcon, PlusCircleIcon, TerminalIcon } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { useShell } from "../../layouts/shell";
import { sampleSessions } from "../../lib/sample/sessions";
import { useTaskName } from "./useTaskName";

const destinations = [
  { label: "Live execution", Icon: TerminalIcon, page: "execution" },
  { label: "Review changes", Icon: GitCompareIcon, page: "review" },
  { label: "Checkpoints", Icon: HistoryIcon, page: "checkpoints" },
] as const;

export function AgentSidebar() {
  const { route, navigate } = useNavigation();
  const shell = useShell();
  const taskName = useTaskName();

  return (
    <SectionSidebar
      title="Agent"
      action={{ label: "New agent task", icon: <PlusCircleIcon />, onClick: shell.startNewSession }}
    >
      <SidebarSection title="Active task">
        <SidebarItem label={taskName} icon={<BotIcon />} active />
      </SidebarSection>
      <SidebarSection title="Navigate">
        {destinations.map(({ label, Icon, page }) => (
          <SidebarItem key={page} label={label} icon={<Icon />} onSelect={() => navigate({ page })} />
        ))}
      </SidebarSection>
      <SidebarSection title="Recent runs">
        {sampleSessions
          .filter((session) => session.kind === "agent" && session.id !== route.id)
          .map((session) => (
            <SidebarItem
              key={session.id}
              label={session.title}
              size="sm"
              onSelect={() => navigate({ page: "agent", id: session.id })}
            />
          ))}
      </SidebarSection>
    </SectionSidebar>
  );
}
