import { useState } from "react";
import { List, ListRow, Notice, PageHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { Button, SearchField, Stack, Text } from "@aidex/ui/primitives";
import { call } from "@aidex/client/rpc";
import { toAidexError } from "@aidex/client/errors";
import { useRpc } from "../../hooks/useRpc";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";

export function WorkspacesPage() {
  const workspaces = useRpc("workspace.list");
  const { navigate } = useNavigation();
  const { setWorkspaceId } = usePreferences();
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string>();
  const items = (workspaces.data?.workspaces ?? []).filter((workspace) =>
    `${workspace.name} ${workspace.path}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  return (
    <PageFrame width="default">
      <Stack gap={5}>
        <PageHeader
          title="Workspaces"
          description="Registered projects on the drive and this computer."
          actions={
            <Button
              variant="primary"
              onClick={() => navigate({ page: "workspace-create" })}
            >
              Add workspace
            </Button>
          }
        />
        <SearchField
          label="Search workspaces"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        {(error || workspaces.error) && (
          <Notice tone="danger">{error ?? workspaces.error?.message}</Notice>
        )}
        <List>
          {items.map((workspace) => (
            <ListRow
              key={workspace.id}
              title={workspace.name}
              subtitle={workspace.path}
              meta={[workspace.kind.type, workspace.availability.state]}
              onSelect={() => {
                setWorkspaceId(workspace.id);
                void call("workspace.select", { id_or_name: workspace.id })
                  .then(() => navigate({ page: "workspace", id: workspace.id }))
                  .catch((caught) => setError(toAidexError(caught).message));
              }}
              trailing={
                <Button
                  variant="ghost"
                  onClick={() => {
                    if (
                      !window.confirm(
                        `Remove ${workspace.name} from AiDEX? Project files will be kept.`,
                      )
                    )
                      return;
                    void call("workspace.remove", {
                      id: workspace.id,
                      delete_data: false,
                    })
                      .then(workspaces.refetch)
                      .catch((caught) =>
                        setError(toAidexError(caught).message),
                      );
                  }}
                >
                  Remove registration
                </Button>
              }
            />
          ))}
        </List>
        {!workspaces.loading && !items.length && (
          <Text tone="muted">
            No workspaces found. Add a project to start an agent task.
          </Text>
        )}
      </Stack>
    </PageFrame>
  );
}
