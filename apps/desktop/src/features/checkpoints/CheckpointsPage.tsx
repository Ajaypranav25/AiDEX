import { EmptyState, PageHeader, SectionHeader, Timeline } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { HistoryIcon, PlusIcon } from "@aidex/ui/icons";
import { Button, Split, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useToast } from "../../app/providers/toast";
import { workspaceById } from "../../lib/sample/workspaces";
import { CheckpointDetail } from "./CheckpointDetail";
import { checkpointsFor, selectedCheckpoint } from "./sample";

const SAVE_PREVIEW = "Preview: saving checkpoints will be available when AiDEX Core is connected.";

/** Checkpoints of the current workspace (preference); the route `id` selects one, shared with the sidebar. */
export function CheckpointsPage() {
  const { route, replace } = useNavigation();
  const { workspaceId } = usePreferences();
  const notify = useToast();
  const workspace = workspaceById(workspaceId);
  const checkpoints = checkpointsFor(workspace.id);
  const selected = selectedCheckpoint(workspace.id, route.id);

  const saveCheckpoint = () => notify(SAVE_PREVIEW);

  return (
    <PageFrame width="default">
      <PageHeader
        title="Checkpoints"
        description="Resume, restore, or branch from a saved workspace state."
        actions={
          <Button variant="primary" size="md" icon={<PlusIcon />} onClick={saveCheckpoint}>
            Save checkpoint
          </Button>
        }
      />
      {selected ? (
        <Split divided emphasis="end">
          <section>
            <Stack gap={4}>
              <SectionHeader title="Timeline" meta={`${checkpoints.length} saved`} />
              <Timeline
                aria-label={`Checkpoints for ${workspace.name}`}
                items={checkpoints.map(({ id, title, time }) => ({ id, title, time }))}
                selectedId={selected.id}
                onSelect={(id) => replace({ ...route, id })}
              />
            </Stack>
          </section>
          <CheckpointDetail key={selected.id} checkpoint={selected} workspaceName={workspace.name} />
        </Split>
      ) : (
        <EmptyState
          icon={<HistoryIcon size={28} />}
          title={`No checkpoints for ${workspace.name} yet`}
          description="Save a checkpoint before a risky change so you can resume or restore from it later."
          actions={
            <Button variant="outline" size="md" icon={<PlusIcon />} onClick={saveCheckpoint}>
              Save checkpoint
            </Button>
          }
        />
      )}
    </PageFrame>
  );
}
