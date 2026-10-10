import { useState } from "react";
import { EmptyState, List, ListRow, PageHeader, SectionHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { PinIcon, PlusIcon, SearchIcon } from "@aidex/ui/icons";
import { Badge, Button, IconBadge, SearchField, SegmentedControl, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { sampleWorkspaces } from "../../lib/sample/workspaces";
import { locationLabel } from "./sample";
import { filterOf, matchesWorkspace, workspaceFilterOptions, type WorkspaceFilter } from "./workspaceFilters";

const sectionTitleByFilter: Record<WorkspaceFilter, string> = {
  all: "All workspaces",
  pinned: "Pinned workspaces",
  drive: "On the AiDEX drive",
  host: "On this computer",
};

export function WorkspacesPage() {
  const { route, navigate, replace } = useNavigation();
  const { setWorkspaceId } = usePreferences();
  const [query, setQuery] = useState("");
  const filter = filterOf(route.tab);
  const workspaces = sampleWorkspaces.filter((workspace) => matchesWorkspace(workspace, filter, query));

  const setFilter = (next: WorkspaceFilter) => replace({ ...route, tab: next });
  const openWorkspace = (id: string) => {
    setWorkspaceId(id);
    navigate({ page: "workspace", id });
  };
  const clearFilters = () => {
    setQuery("");
    setFilter("all");
  };

  return (
    <PageFrame width="default">
      <PageHeader
        title="Workspaces"
        description="Open projects from your computer or AiDEX drive."
        actions={
          <Button variant="primary" size="md" icon={<PlusIcon />} onClick={() => navigate({ page: "workspace-create" })}>
            New workspace
          </Button>
        }
      />
      <Stack direction="row" gap={3} align="center" justify="between" wrap>
        <SearchField
          label="Search workspaces"
          placeholder="Search by name, path, or language"
          size="md"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <SegmentedControl
          label="Show workspaces"
          size="sm"
          options={workspaceFilterOptions}
          value={filter}
          onValueChange={setFilter}
        />
      </Stack>
      <section aria-labelledby="workspaces-list-title">
        <SectionHeader
          title={<span id="workspaces-list-title">{sectionTitleByFilter[filter]}</span>}
          meta={`${workspaces.length} ${workspaces.length === 1 ? "workspace" : "workspaces"}`}
        />
        {workspaces.length > 0 ? (
          <List metaWidth="128px">
            {workspaces.map(({ id, name, path, location, lastOpened, pinned, Icon, iconTone }) => (
              <ListRow
                key={id}
                media={<IconBadge size="md" tone={iconTone === "accent" ? "accent" : "neutral"} icon={<Icon />} />}
                title={
                  <>
                    {name}{" "}
                    {pinned ? (
                      <Badge tone="neutral" icon={<PinIcon />}>
                        Pinned
                      </Badge>
                    ) : null}
                  </>
                }
                subtitle={path}
                meta={[locationLabel[location], `Opened ${lastOpened.toLowerCase()}`]}
                onSelect={() => openWorkspace(id)}
              />
            ))}
          </List>
        ) : (
          <EmptyState
            icon={<SearchIcon size={28} />}
            title="No matching workspaces"
            description="Try another name or path, or show every workspace."
            actions={
              <Button variant="outline" size="md" onClick={clearFilters}>
                Clear search and filters
              </Button>
            }
          />
        )}
      </section>
    </PageFrame>
  );
}
