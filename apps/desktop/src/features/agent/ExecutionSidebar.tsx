import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { BotIcon, CodeIcon, FlaskIcon, GitBranchIcon, GitCompareIcon, TerminalIcon } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { tabOf } from "../../lib/routes";
import { DEFAULT_TASK, runAgents } from "./sample";

const agentIcons = { main: <BotIcon />, code: <CodeIcon />, test: <FlaskIcon /> } as const;

export function ExecutionSidebar() {
  const { route, navigate, replace } = useNavigation();
  const tab = tabOf(route, "execution");
  const showTab = (value: "execution" | "graph") => replace({ ...route, tab: value });

  return (
    <SectionSidebar title="Execution">
      <SidebarSection title="Active run">
        <SidebarItem label={DEFAULT_TASK} icon={<BotIcon />} onSelect={() => navigate({ page: "agent" })} />
        <SidebarItem label="Execution" icon={<TerminalIcon />} active={tab === "execution"} onSelect={() => showTab("execution")} />
        <SidebarItem label="Task graph" icon={<GitBranchIcon />} active={tab === "graph"} onSelect={() => showTab("graph")} />
        <SidebarItem label="Review changes" icon={<GitCompareIcon />} onSelect={() => navigate({ page: "review" })} />
      </SidebarSection>
      {/* Counts are tool calls each worker made; selecting one opens the graph. */}
      <SidebarSection title="Agents">
        {runAgents.map(({ id, label, calls }) => (
          <SidebarItem key={id} label={label} icon={agentIcons[id]} count={calls} onSelect={() => showTab("graph")} />
        ))}
      </SidebarSection>
    </SectionSidebar>
  );
}
