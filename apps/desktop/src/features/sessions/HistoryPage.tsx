import { useState } from "react";
import { EmptyState, PageHeader, SectionHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { HistoryIcon, XIcon } from "@aidex/ui/icons";
import { Badge, Button, SearchField, SegmentedControl, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import {
  groupByDay,
  kindFilterOf,
  kindFilterOptions,
  matchesKind,
  matchesQuery,
  sessionCountLabel,
  sessionsInWorkspace,
  workspaceFilterOf,
  type KindFilter,
} from "./historyFilters";
import { SessionDayGroup } from "./SessionDayGroup";

export function HistoryPage() {
  const { route, replace } = useNavigation();
  const [query, setQuery] = useState("");
  const kind = kindFilterOf(route);
  const workspace = workspaceFilterOf(route);

  const sessions = sessionsInWorkspace(workspace?.id).filter(
    (session) => matchesKind(session, kind) && matchesQuery(session, query),
  );
  const groups = groupByDay(sessions);

  const setKind = (next: KindFilter) => replace({ ...route, tab: next });
  const clearWorkspace = () => replace({ ...route, id: undefined });
  const clearFilters = () => {
    setQuery("");
    replace({ page: "history" });
  };

  return (
    <PageFrame width="default">
      <PageHeader
        title="Session history"
        description="Every conversation and run in one place."
        meta={
          workspace ? (
            <Stack direction="row" gap={2} align="center" wrap>
              <Badge tone="neutral">Workspace: {workspace.name}</Badge>
              <Button variant="ghost" size="md" icon={<XIcon />} onClick={clearWorkspace}>
                Show all workspaces
              </Button>
            </Stack>
          ) : undefined
        }
      />
      <Stack direction="row" gap={3} align="center" justify="between" wrap>
        <SearchField
          label="Search history"
          placeholder="Search history..."
          size="md"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <SegmentedControl label="Session type" size="sm" options={kindFilterOptions} value={kind} onValueChange={setKind} />
      </Stack>
      <section>
        <Stack gap={6}>
          <SectionHeader title="Sessions" meta={sessionCountLabel(sessions.length)} />
          {groups.length === 0 ? (
            <EmptyState
              icon={<HistoryIcon size={28} />}
              title="No sessions match these filters"
              description="Try a different search, or clear the filters to see every chat and agent run."
              actions={
                <Button variant="outline" size="md" onClick={clearFilters}>
                  Clear filters
                </Button>
              }
            />
          ) : (
            groups.map((group) => <SessionDayGroup key={group.label} group={group} />)
          )}
        </Stack>
      </section>
    </PageFrame>
  );
}
