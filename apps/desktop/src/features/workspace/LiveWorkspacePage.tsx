import { useState } from "react";
import {
  CodeBlock,
  KeyValueList,
  List,
  ListRow,
  Notice,
  PageHeader,
} from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import {
  Button,
  Card,
  IconButton,
  SearchField,
  Stack,
  Tabs,
  Text,
  TextField,
} from "@aidex/ui/primitives";
import { ArrowLeftIcon, FolderIcon, FileCodeIcon } from "@aidex/ui/icons";
import { callUntyped } from "@aidex/client/rpc";
import { toAidexError } from "@aidex/client/errors";
import { useDomain } from "../../hooks/useDomain";
import { useRpc } from "../../hooks/useRpc";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import type { HostProfile } from "@aidex/client/dto/host";

type Tree = {
  entries: { path: string; kind: string; size: number | null }[];
  truncated: boolean;
};
type File = {
  path: string;
  content: string | null;
  binary: boolean;
  truncated: boolean;
  size: number;
  start_line: number;
  end_line: number;
  next_line: number | null;
};
type Memory = {
  id: string;
  workspace_id: string;
  key: string;
  content: string;
  updated_at: number;
};
type GitStatus = {
  branch: {
    head: string | null;
    upstream: string | null;
    ahead: number | null;
    behind: number | null;
  };
  entries: { path: string; index: string; worktree: string; kind: string }[];
  truncated: boolean;
};
type GitCommit = {
  oid: string;
  subject: string;
  author_name: string;
  authored_at: string;
};

export function LiveWorkspacePage() {
  const { route, replace, navigate } = useNavigation();
  const prefs = usePreferences();
  const workspaceId = route.id || prefs.workspaceId;
  const workspace = useRpc(
    "workspace.get",
    { id: workspaceId },
    { enabled: !!workspaceId },
  );
  const tab = route.tab ?? "overview";
  const [directory, setDirectory] = useState("");
  const tree = useDomain<Tree>(
    "workspace.files.tree",
    {
      workspace_id: workspaceId,
      path: directory || undefined,
      recursive: false,
      max_entries: 500,
    },
    !!workspaceId && ["overview", "files"].includes(tab),
  );
  const [path, setPath] = useState("");
  const [fileLine, setFileLine] = useState(1);
  const openFile = (path: string, line = 1) => {
    setPath(path);
    setFileLine(line);
  };
  const file = useDomain<File>(
    "workspace.files.read",
    {
      workspace_id: workspaceId,
      path,
      start_line: fileLine,
      max_lines: 200,
      max_bytes: 262144,
    },
    !!workspaceId && !!path && tab === "files",
  );
  const git = useDomain<GitStatus>(
    "workspace.git.status",
    { workspace_id: workspaceId },
    !!workspaceId && tab === "git",
  );
  const environment = useDomain<HostProfile>(
    "workspace.environment",
    undefined,
    !!workspaceId && tab === "environment",
  );
  const commits = useDomain<{ commits: GitCommit[] }>(
    "workspace.git.log",
    { workspace_id: workspaceId, max_count: 20 },
    !!workspaceId && tab === "git",
  );
  const memories = useDomain<{ memories: Memory[] }>(
    "native.memory.list",
    { workspace_id: workspaceId },
    !!workspaceId && tab === "memory",
  );
  const sessions = useRpc("conversation.list", undefined, {
    enabled: tab === "sessions",
  });
  const [memoryKey, setMemoryKey] = useState("");
  const [memoryText, setMemoryText] = useState("");
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState<{
    matches: { path: string; line: number; text: string }[];
    truncated: boolean;
  }>();
  const [error, setError] = useState<string>();
  const act = async (job: () => Promise<unknown>) => {
    setError(undefined);
    try {
      await job();
    } catch (caught) {
      setError(toAidexError(caught).message);
    }
  };
  const errorMessage =
    error ??
    workspace.error?.message ??
    tree.error?.message ??
    file.error?.message ??
    git.error?.message ??
    environment.error?.message ??
    commits.error?.message ??
    memories.error?.message;
  return (
    <PageFrame width="default">
      <Stack gap={5}>
        <PageHeader
          title={workspace.data?.name ?? "Workspace"}
          description={workspace.data?.path ?? "Choose a registered project."}
          actions={
            <Button
              variant="primary"
              disabled={!workspaceId}
              onClick={() => {
                prefs.setPendingWorkspaceId(workspaceId);
                prefs.setWorkspaceId(workspaceId);
                navigate({ page: "agent" });
              }}
            >
              Start agent task
            </Button>
          }
        />
        {errorMessage && <Notice tone="danger">{errorMessage}</Notice>}
        {!workspaceId && (
          <Button
            variant="outline"
            onClick={() => navigate({ page: "workspaces" })}
          >
            Choose workspace
          </Button>
        )}
        <Tabs
          label="Workspace sections"
          id="live-workspace-tabs"
          variant="underline"
          value={tab}
          items={[
            "overview",
            "files",
            "git",
            "environment",
            "sessions",
            "memory",
          ].map((value) => ({
            value,
            label:
              value === "memory"
                ? "Context & memory"
                : value[0].toUpperCase() + value.slice(1),
          }))}
          onValueChange={(tab) => replace({ ...route, tab })}
        />
        {tab === "overview" && workspace.data && (
          <KeyValueList
            items={[
              { label: "Location", value: workspace.data.kind.type },
              { label: "Path", value: workspace.data.path },
              {
                label: "Availability",
                value: workspace.data.availability.state,
              },
              {
                label: "Repository",
                value: workspace.data.repository?.vcs ?? "none",
              },
              {
                label: "Listed entries",
                value: String(tree.data?.entries.length ?? 0),
              },
            ]}
          />
        )}
        {tab === "files" && (
          <Stack gap={4}>
            <Stack direction="row" gap={2}>
              <SearchField
                label="Search project contents"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
              <Button
                variant="outline"
                disabled={!query.trim()}
                onClick={() =>
                  void act(async () =>
                    setSearch(
                      (await callUntyped("workspace.files.search", {
                        workspace_id: workspaceId,
                        pattern: query,
                        max_results: 100,
                      })) as typeof search,
                    ),
                  )
                }
              >
                Search
              </Button>
            </Stack>
            {search && (
              <List>
                {search.matches.map((match, index) => (
                  <ListRow
                    key={index}
                    title={`${match.path}:${match.line}`}
                    subtitle={match.text}
                    onSelect={() =>
                      openFile(match.path, Math.max(1, match.line - 3))
                    }
                  />
                ))}
              </List>
            )}
            <Stack direction="row" gap={2} align="center">
              <IconButton
                size="sm"
                label="Parent folder"
                icon={<ArrowLeftIcon />}
                disabled={!directory}
                onClick={() => {
                  setDirectory(directory.split("/").slice(0, -1).join("/"));
                  setPath("");
                }}
              />
              <Text>{directory || "Project root"}</Text>
            </Stack>
            <List>
              {(tree.data?.entries ?? []).map((entry) => (
                <ListRow
                  key={entry.path}
                  title={entry.path.split("/").at(-1) || entry.path}
                  media={
                    entry.kind === "file" ? <FileCodeIcon /> : <FolderIcon />
                  }
                  meta={[
                    entry.kind,
                    entry.size === null ? "" : `${entry.size} bytes`,
                  ]}
                  onSelect={() => {
                    if (entry.kind === "file") openFile(entry.path);
                    else if (["directory", "dir"].includes(entry.kind)) {
                      setDirectory(entry.path);
                      setPath("");
                    }
                  }}
                />
              ))}
            </List>
            {tree.data?.truncated && (
              <Notice>
                Directory listing reached its bound. Search for a specific file.
              </Notice>
            )}
            {file.data && (
              <Card padding="md">
                <Stack gap={3}>
                  <Text>{file.data.path}</Text>
                  {file.data.binary ? (
                    <Text>Binary file · {file.data.size} bytes</Text>
                  ) : (
                    <CodeBlock
                      title={file.data.path}
                      code={file.data.content ?? ""}
                    />
                  )}
                  {file.data.truncated && (
                    <Notice>File preview is truncated.</Notice>
                  )}
                  {!file.data.binary && (
                    <Stack direction="row" gap={2} align="center">
                      <Button
                        variant="ghost"
                        disabled={fileLine === 1}
                        onClick={() => setFileLine(Math.max(1, fileLine - 200))}
                      >
                        Previous lines
                      </Button>
                      <Text tone="muted">
                        Lines {file.data.start_line}–{file.data.end_line}
                      </Text>
                      <Button
                        variant="ghost"
                        disabled={file.data.next_line == null}
                        onClick={() => setFileLine(file.data!.next_line!)}
                      >
                        Next lines
                      </Button>
                    </Stack>
                  )}
                  <Button
                    variant="outline"
                    disabled={file.data.binary}
                    onClick={() => {
                      prefs.setPendingWorkspaceId(workspaceId);
                      prefs.setPendingPrompt(
                        `Explain this project file ${file.data!.path}${file.data!.truncated ? " (excerpt)" : ""}:\n\n${file.data!.content ?? ""}`,
                      );
                      navigate({ page: "chat" });
                    }}
                  >
                    Ask about this file
                  </Button>
                </Stack>
              </Card>
            )}
          </Stack>
        )}
        {tab === "git" && (
          <Stack gap={4}>
            <KeyValueList
              items={[
                { label: "Branch", value: git.data?.branch.head ?? "Unknown" },
                {
                  label: "Upstream",
                  value: git.data?.branch.upstream ?? "None",
                },
              ]}
            />
            <List>
              {git.data?.entries.map((entry) => (
                <ListRow
                  key={entry.path}
                  title={entry.path}
                  meta={[
                    entry.kind,
                    `Staged ${entry.index} · working tree ${entry.worktree}`,
                  ]}
                  onSelect={() => {
                    setPath(entry.path);
                    replace({ ...route, tab: "files" });
                  }}
                />
              ))}
            </List>
            {git.data && !git.data.entries.length && (
              <Text tone="muted">Working tree is clean.</Text>
            )}
            <List>
              {commits.data?.commits.map((commit) => (
                <ListRow
                  key={commit.oid}
                  title={commit.subject}
                  subtitle={`${commit.oid.slice(0, 8)} · ${commit.author_name}`}
                  meta={[commit.authored_at]}
                />
              ))}
            </List>
          </Stack>
        )}
        {tab === "environment" && environment.data && (
          <KeyValueList
            items={[
              { label: "Operating system", value: environment.data.os },
              { label: "Architecture", value: environment.data.architecture },
              {
                label: "CPU threads",
                value: String(environment.data.logical_cpu_count ?? "Unknown"),
              },
              {
                label: "Git",
                value:
                  environment.data.git.state === "available"
                    ? "Available"
                    : "Not detected",
              },
              {
                label: "Shells",
                value:
                  environment.data.shells
                    .filter((shell) => shell.result.state === "available")
                    .map((shell) => shell.name)
                    .join(", ") || "None detected",
              },
              {
                label: "Rust",
                value:
                  environment.data.rust_toolchain.cargo.state === "available"
                    ? "Available"
                    : "Not detected",
              },
            ]}
          />
        )}
        {tab === "sessions" && (
          <List>
            {(sessions.data?.sessions ?? [])
              .filter((session) => session.workspace_id === workspaceId)
              .map((session) => (
                <ListRow
                  key={session.id}
                  title={session.title ?? "Locked session"}
                  meta={[session.status]}
                  onSelect={() =>
                    navigate({
                      page: session.mode === "agent" ? "agent" : "chat",
                      id: session.id,
                    })
                  }
                />
              ))}
          </List>
        )}
        {tab === "memory" && (
          <Stack gap={4}>
            <Text>
              Saved project memory is encrypted on the drive and supplied as
              bounded context to native tasks.
            </Text>
            <List>
              {(memories.data?.memories ?? []).map((memory) => (
                <ListRow
                  key={memory.id}
                  title={memory.key}
                  subtitle={memory.content}
                  onSelect={() => {
                    setMemoryKey(memory.key);
                    setMemoryText(memory.content);
                  }}
                  trailing={
                    <Button
                      variant="ghost"
                      onClick={() =>
                        void act(async () => {
                          await callUntyped("native.memory.delete", {
                            workspace_id: workspaceId,
                            id: memory.id,
                          });
                          memories.refetch();
                        })
                      }
                    >
                      Delete entry
                    </Button>
                  }
                />
              ))}
            </List>
            <TextField
              label="Memory key"
              value={memoryKey}
              onChange={(event) => setMemoryKey(event.target.value)}
            />
            <TextField
              label="Memory content"
              value={memoryText}
              onChange={(event) => setMemoryText(event.target.value)}
            />
            <Button
              variant="primary"
              onClick={() =>
                void act(async () => {
                  await callUntyped("native.memory.set", {
                    workspace_id: workspaceId,
                    key: memoryKey,
                    content: memoryText,
                  });
                  memories.refetch();
                })
              }
            >
              Save memory
            </Button>
          </Stack>
        )}
      </Stack>
    </PageFrame>
  );
}
