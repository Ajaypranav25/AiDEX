import { FileTree, KeyValueList, List, ListRow, SectionHeader, StatTile } from "@aidex/ui/components";
import { ChevronRightIcon, FileCodeIcon, GitCommitIcon, GitCompareIcon, HistoryIcon } from "@aidex/ui/icons";
import { Button, Grid, Split, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import type { SampleWorkspace } from "../../lib/sample/workspaces";
import { indexStatusFor, recentActivity } from "./sample";
import type { ProjectProfile } from "./sampleProjects";
import type { WorkspaceTab } from "./workspaceTabs";

const ENVIRONMENT_SUMMARY_LIMIT = 3;

type Props = {
  workspace: SampleWorkspace;
  profile: ProjectProfile;
  sessionCount: number;
  onTabChange: (tab: WorkspaceTab) => void;
};

/** Workspace dashboard: headline stats, a file preview, recent activity, and the detected environment. */
export function OverviewPanel({ workspace, profile, sessionCount, onTabChange }: Props) {
  const { navigate } = useNavigation();
  const index = indexStatusFor(workspace.id);
  const [lastCommit] = profile.commits;

  const viewAll = (tab: WorkspaceTab, label: string, accessibleName: string) => (
    <Button variant="ghost" size="md" trailing={<ChevronRightIcon />} aria-label={accessibleName} onClick={() => onTabChange(tab)}>
      {label}
    </Button>
  );

  return (
    <Stack gap={8}>
      <Grid columns={4} gap={4}>
        <StatTile
          label="Files indexed"
          value={index.filesIndexed.toLocaleString("en-US")}
          hint={`of ${index.filesTotal.toLocaleString("en-US")} files`}
          icon={<FileCodeIcon />}
        />
        <StatTile
          label="Open changes"
          value={profile.changes.length}
          hint={`Uncommitted on ${workspace.branch}`}
          icon={<GitCompareIcon />}
        />
        <StatTile label="Last commit" value={lastCommit?.time ?? "None"} hint={lastCommit?.message} icon={<GitCommitIcon />} />
        <StatTile label="Sessions" value={sessionCount} hint="Chats and agent runs" icon={<HistoryIcon />} />
      </Grid>
      <Split divided>
        <section aria-labelledby="overview-files-title">
          <Stack gap={3}>
            <SectionHeader title={<span id="overview-files-title">Files</span>} actions={viewAll("files", "View all", "View all files")} />
            <FileTree aria-label={`${workspace.name} files`} nodes={profile.tree} defaultExpanded={profile.expanded} />
          </Stack>
        </section>
        <Stack gap={8}>
          <section aria-labelledby="overview-activity-title">
            <Stack gap={3}>
              <SectionHeader
                title={<span id="overview-activity-title">Recent activity</span>}
                actions={
                  <Button
                    variant="ghost"
                    size="md"
                    trailing={<ChevronRightIcon />}
                    aria-label="Open session history"
                    onClick={() => navigate({ page: "history" })}
                  >
                    History
                  </Button>
                }
              />
              <List metaWidth="96px">
                {recentActivity.map((entry) => (
                  <ListRow key={entry.id} title={entry.title} meta={[entry.time]} />
                ))}
              </List>
            </Stack>
          </section>
          <section aria-labelledby="overview-environment-title">
            <Stack gap={3}>
              <SectionHeader
                title={<span id="overview-environment-title">Environment</span>}
                actions={viewAll("environment", "Details", "Environment details")}
              />
              <KeyValueList
                items={profile.toolchains
                  .slice(0, ENVIRONMENT_SUMMARY_LIMIT)
                  .map((tool) => ({ label: tool.name, value: tool.version }))}
              />
            </Stack>
          </section>
        </Stack>
      </Split>
    </Stack>
  );
}
