import { EmptyState, List, ListRow, SectionHeader } from "@aidex/ui/components";
import { CheckCircleIcon, FileCodeIcon, GitBranchIcon, GitCommitIcon, GitCompareIcon, ShieldCheckIcon } from "@aidex/ui/icons";
import { Badge, Button, Stack, StatusIndicator, Text } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import type { SampleWorkspace } from "../../lib/sample/workspaces";
import type { ProjectProfile } from "./sampleProjects";

type Props = { workspace: SampleWorkspace; profile: ProjectProfile };

/** Current branch, uncommitted changes, and recent commits. */
export function GitPanel({ workspace, profile }: Props) {
  const { navigate } = useNavigation();
  const changeCount = profile.changes.length;

  return (
    <Stack gap={8}>
      <Stack direction="row" gap={3} align="center" wrap>
        <Badge tone="neutral" icon={<GitBranchIcon />}>
          {workspace.branch}
        </Badge>
        <StatusIndicator
          tone={changeCount > 0 ? "warning" : "success"}
          label={changeCount > 0 ? `${changeCount} uncommitted ${changeCount === 1 ? "change" : "changes"}` : "Working tree clean"}
        />
      </Stack>
      <section aria-labelledby="git-changes-title">
        <Stack gap={3}>
          <SectionHeader
            title={<span id="git-changes-title">Changed files</span>}
            meta={`${changeCount} ${changeCount === 1 ? "file" : "files"}`}
            actions={
              changeCount > 0 ? (
                <Button variant="outline" size="md" icon={<GitCompareIcon />} onClick={() => navigate({ page: "review" })}>
                  Review changes
                </Button>
              ) : null
            }
          />
          {changeCount > 0 ? (
            <List metaWidth="96px">
              {profile.changes.map((change) => (
                <ListRow
                  key={change.path}
                  media={<FileCodeIcon />}
                  title={change.path}
                  meta={[change.status, `+${change.additions} −${change.deletions}`]}
                  onSelect={() => navigate({ page: "review" })}
                />
              ))}
            </List>
          ) : (
            <EmptyState icon={<CheckCircleIcon size={28} />} title="No uncommitted changes" />
          )}
        </Stack>
      </section>
      <section aria-labelledby="git-commits-title">
        <Stack gap={3}>
          <SectionHeader title={<span id="git-commits-title">Recent commits</span>} />
          <List metaWidth="96px">
            {profile.commits.map((commit) => (
              <ListRow
                key={commit.hash}
                media={<GitCommitIcon />}
                title={commit.message}
                subtitle={`${commit.hash} · ${commit.author}`}
                meta={[commit.time]}
              />
            ))}
          </List>
        </Stack>
      </section>
      <Stack direction="row" gap={2} align="center">
        <ShieldCheckIcon size={16} />
        <Text as="span" tone="muted">
          Changes are reviewed before an agent commits or pushes them.
        </Text>
      </Stack>
    </Stack>
  );
}
