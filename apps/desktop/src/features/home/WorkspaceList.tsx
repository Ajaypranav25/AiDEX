import { EmptyState, List, ListRow } from "@aidex/ui/components";
import { FolderIcon, HardDriveIcon, LayersIcon } from "@aidex/ui/icons";
import { IconBadge, StatusIndicator } from "@aidex/ui/primitives";
import type { Workspace, WorkspaceList as WorkspaceRegistry } from "@aidex/client/dto/workspace";
import { useRpc } from "../../hooks/useRpc";
import { QueryState } from "../system/QueryState";

const KIND_LABEL = { host: "Folder on this computer", drive: "On the AiDEX drive", ephemeral: "Ephemeral copy" } as const;
const KIND_ICON = { host: FolderIcon, drive: HardDriveIcon, ephemeral: LayersIcon } as const;

/** Most recently opened first; never-opened workspaces by creation time. */
const byRecency = (left: Workspace, right: Workspace) =>
  (right.last_opened_at ?? right.created_at).localeCompare(left.last_opened_at ?? left.created_at);

/** The drive's workspace registry from `workspace.list`. */
export function WorkspaceList({ limit = 6 }: { limit?: number }) {
  const list = useRpc("workspace.list");
  return (
    <QueryState query={list} loading="Loading workspaces…" failure="Cannot list workspaces">
      {(registry) => <Workspaces registry={registry} limit={limit} />}
    </QueryState>
  );
}

function Workspaces({ registry, limit }: { registry: WorkspaceRegistry; limit: number }) {
  const workspaces = [...registry.workspaces].sort(byRecency).slice(0, limit);
  if (workspaces.length === 0) {
    return (
      <EmptyState
        icon={<FolderIcon />}
        title="No workspaces yet"
        description="Add a project folder with `aidex workspace add <path>`; adding from the desktop arrives with the workspace screens."
      />
    );
  }
  return (
    <List>
      {workspaces.map((workspace) => {
        const Icon = KIND_ICON[workspace.kind.type];
        const available = workspace.availability.state === "available";
        const selected = workspace.id === registry.selected;
        return (
          <ListRow
            key={workspace.id}
            media={<IconBadge size="md" icon={<Icon />} />}
            title={workspace.name}
            subtitle={`${KIND_LABEL[workspace.kind.type]} · ${workspace.path}${workspace.repository?.vcs === "git" ? " · git" : ""}`}
            trailing={
              <StatusIndicator
                tone={available ? (selected ? "success" : "neutral") : "warning"}
                label={!available ? "Unavailable" : selected ? "Selected" : "Available"}
              />
            }
          />
        );
      })}
    </List>
  );
}
