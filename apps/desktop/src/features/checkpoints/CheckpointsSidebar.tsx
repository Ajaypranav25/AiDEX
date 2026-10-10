import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { BotIcon, GitCompareIcon, HistoryIcon } from "@aidex/ui/icons";
import { Select } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { sampleWorkspaces, workspaceById } from "../../lib/sample/workspaces";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { checkpointsFor, selectedCheckpoint } from "./sample";

const workspaceOptions = sampleWorkspaces.map(({ id, name }) => ({ value: id, label: name }));

/** Recovery navigation: workspace picker, that workspace's checkpoints (route `id`), and related screens. */
export function CheckpointsSidebar() {
  const { route, navigate, replace } = useNavigation();
  const { workspaceId, setWorkspaceId } = usePreferences();
  const workspace = workspaceById(workspaceId);
  const checkpoints = checkpointsFor(workspace.id);
  const selected = selectedCheckpoint(workspace.id, route.id);

  const changeWorkspace = (id: string) => {
    setWorkspaceId(id);
    // The old selection belongs to the previous workspace.
    replace({ page: "checkpoints" });
  };

  return (
    <SectionSidebar
      title="Recovery"
      headerExtra={
        <Select
          label="Workspace"
          labelVisibility="hidden"
          options={workspaceOptions}
          value={workspace.id}
          onChange={(event) => changeWorkspace(event.target.value)}
        />
      }
    >
      {/* With no checkpoints the page shows its own empty state, so the section is left out. */}
      {checkpoints.length > 0 ? (
        <SidebarSection title="Checkpoints">
          {checkpoints.map((checkpoint) => (
            <SidebarItem
              key={checkpoint.id}
              label={checkpoint.title}
              icon={<HistoryIcon />}
              active={checkpoint.id === selected?.id}
              onSelect={() => replace({ page: "checkpoints", id: checkpoint.id })}
            />
          ))}
        </SidebarSection>
      ) : null}
      <SidebarSection title="Other">
        <SidebarItem label="Review current changes" icon={<GitCompareIcon />} onSelect={() => navigate({ page: "review" })} />
        <SidebarItem label="Agent task" icon={<BotIcon />} onSelect={() => navigate({ page: "agent" })} />
      </SidebarSection>
    </SectionSidebar>
  );
}
