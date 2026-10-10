import { useState } from "react";
import { KeyValueList, Meter, SectionHeader, SettingRow } from "@aidex/ui/components";
import { RefreshIcon } from "@aidex/ui/icons";
import { Button, Split, Stack, Switch } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import type { SampleWorkspace } from "../../lib/sample/workspaces";
import { MemoryEntries } from "./MemoryEntries";
import { RetrievalTest } from "./RetrievalTest";
import { EMBEDDING_MODEL, indexStatusFor } from "./sample";
import type { ProjectProfile } from "./sampleProjects";

type Props = { workspace: SampleWorkspace; profile: ProjectProfile };

type SettingKey = "autoIndex" | "gitHistory" | "shareMemory";

const settings: readonly { key: SettingKey; title: string; description: string }[] = [
  { key: "autoIndex", title: "Auto-index on change", description: "Update the index when files in this workspace change." },
  { key: "gitHistory", title: "Include git history", description: "Add commit messages and diffs to the searchable context." },
  {
    key: "shareMemory",
    title: "Share memory across workspaces",
    description: "Let other workspaces use the memory saved here.",
  },
];

const formatCount = (value: number) => value.toLocaleString("en-US");

/** Context & memory: index health, a retrieval test, saved memory, and indexing settings. */
export function MemoryPanel({ workspace, profile }: Props) {
  const notify = useToast();
  const index = indexStatusFor(workspace.id);
  const complete = index.filesTotal > 0 && index.filesIndexed >= index.filesTotal;
  const [enabled, setEnabled] = useState<Record<SettingKey, boolean>>({
    autoIndex: true,
    gitHistory: false,
    shareMemory: false,
  });

  return (
    <Stack gap={10}>
      <Split divided>
        <section aria-labelledby="memory-index-title">
          <Stack gap={4}>
            <SectionHeader
              title={<span id="memory-index-title">Repository index</span>}
              meta={complete ? "Up to date" : "Partially indexed"}
              actions={
                <Button
                  variant="outline"
                  size="md"
                  icon={<RefreshIcon />}
                  onClick={() => notify(`Preview: reindexing ${workspace.name} will be available when AiDEX Core is connected.`)}
                >
                  Reindex
                </Button>
              }
            />
            <Meter
              label="Files indexed"
              value={index.filesIndexed}
              max={Math.max(index.filesTotal, 1)}
              valueText={`${formatCount(index.filesIndexed)} of ${formatCount(index.filesTotal)} files`}
              tone={complete ? "success" : "accent"}
            />
            <KeyValueList
              items={[
                { label: "Chunks", value: formatCount(index.chunks) },
                { label: "Embedding model", value: EMBEDDING_MODEL },
                { label: "Last indexed", value: index.lastIndexed },
              ]}
            />
          </Stack>
        </section>
        <RetrievalTest profile={profile} />
      </Split>
      <MemoryEntries workspaceName={workspace.name} />
      <section aria-labelledby="memory-settings-title">
        <Stack gap={3}>
          <SectionHeader title={<span id="memory-settings-title">Context settings</span>} />
          {settings.map(({ key, title, description }) => {
            const controlId = `memory-setting-${key}`;
            return (
              <SettingRow
                key={key}
                title={title}
                description={description}
                controlId={controlId}
                control={
                  <Switch
                    id={controlId}
                    aria-describedby={`${controlId}-description`}
                    checked={enabled[key]}
                    onCheckedChange={(checked) => setEnabled((current) => ({ ...current, [key]: checked }))}
                  />
                }
              />
            );
          })}
        </Stack>
      </section>
    </Stack>
  );
}
