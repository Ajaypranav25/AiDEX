import { useState, type Ref } from "react";
import { Dialog, List, ListRow, Notice } from "@aidex/ui/components";
import { SearchField, Text } from "@aidex/ui/primitives";
import { useRpc } from "../hooks/useRpc";
import { useNavigation } from "../app/providers/navigation";
export function QuickSearch({
  open,
  onClose,
  inputRef,
}: {
  open: boolean;
  onClose: () => void;
  inputRef: Ref<HTMLInputElement>;
}) {
  const [query, setQuery] = useState("");
  const { navigate } = useNavigation();
  const sessions = useRpc("conversation.list", undefined, { enabled: open });
  const workspaces = useRpc("workspace.list", undefined, { enabled: open });
  const needle = query.trim().toLowerCase();
  const matchingSessions = (sessions.data?.sessions ?? [])
    .filter(
      (session) =>
        !session.locked &&
        (!needle ||
          `${session.title ?? ""} ${session.mode ?? ""}`
            .toLowerCase()
            .includes(needle)),
    )
    .slice(0, 8);
  const matchingProjects = (workspaces.data?.workspaces ?? [])
    .filter(
      (workspace) =>
        !needle ||
        `${workspace.name} ${workspace.path}`.toLowerCase().includes(needle),
    )
    .slice(0, 8);
  return (
    <Dialog open={open} title="Search" onClose={onClose}>
      <SearchField
        ref={inputRef}
        autoFocus
        label="Search conversations and projects"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="Conversation title or project name…"
      />
      {(sessions.error || workspaces.error) && (
        <Notice tone="danger">
          {sessions.error?.message ?? workspaces.error?.message}
        </Notice>
      )}
      <List>
        {matchingSessions.map((session) => (
          <ListRow
            key={session.id}
            title={session.title || "Untitled chat"}
            meta={[session.mode ?? "chat"]}
            onSelect={() => {
              onClose();
              navigate({
                page: session.mode === "agent" ? "agent" : "chat",
                id: session.id,
              });
            }}
          />
        ))}
      </List>
      <List>
        {matchingProjects.map((workspace) => (
          <ListRow
            key={workspace.id}
            title={workspace.name}
            subtitle={workspace.path}
            meta={["Project", workspace.availability.state]}
            onSelect={() => {
              onClose();
              navigate({ page: "workspace", id: workspace.id });
            }}
          />
        ))}
      </List>
      {!sessions.loading &&
        !workspaces.loading &&
        !matchingSessions.length &&
        !matchingProjects.length && <Text tone="muted">No results.</Text>}
    </Dialog>
  );
}
