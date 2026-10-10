import { useState } from "react";
import { CodeBlock, EmptyState, FileTree, SectionHeader } from "@aidex/ui/components";
import { ChatIcon, CopyIcon, FileIcon } from "@aidex/ui/icons";
import { Button, IconButton, Split, Stack } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useToast } from "../../app/providers/toast";
import type { SampleWorkspace } from "../../lib/sample/workspaces";
import type { ProjectProfile } from "./sampleProjects";

type Props = { workspace: SampleWorkspace; profile: ProjectProfile };

/** Repository tree with a read-only preview of the selected file. */
export function FilesPanel({ workspace, profile }: Props) {
  const { navigate } = useNavigation();
  const { setWorkspaceId, setPendingPrompt } = usePreferences();
  const notify = useToast();
  const [selectedPath, setSelectedPath] = useState(() => Object.keys(profile.snippets)[0] ?? "");
  const snippet = profile.snippets[selectedPath];

  const copy = (code: string) => {
    if (!navigator.clipboard) {
      notify("Copying isn't available here. Select the code and copy it instead.", "danger");
      return;
    }
    navigator.clipboard.writeText(code).then(
      () => notify(`Copied ${selectedPath} to the clipboard.`, "success"),
      () => notify("Couldn't copy the file. Select the code and copy it instead.", "danger"),
    );
  };

  const askAboutFile = () => {
    setWorkspaceId(workspace.id);
    setPendingPrompt(`Explain what ${selectedPath} does in ${workspace.name}.`);
    navigate({ page: "chat" });
  };

  return (
    <Split divided emphasis="end">
      <section aria-labelledby="files-tree-title">
        <Stack gap={3}>
          <SectionHeader title={<span id="files-tree-title">Repository files</span>} />
          <FileTree
            aria-label={`${workspace.name} files`}
            nodes={profile.tree}
            defaultExpanded={profile.expanded}
            selectedId={selectedPath}
            onSelect={setSelectedPath}
          />
        </Stack>
      </section>
      <section aria-labelledby="files-preview-title">
        <Stack gap={3}>
          <SectionHeader
            title={<span id="files-preview-title">Preview</span>}
            actions={
              selectedPath ? (
                <Button variant="outline" size="md" icon={<ChatIcon />} onClick={askAboutFile}>
                  Ask about this file
                </Button>
              ) : null
            }
          />
          {snippet ? (
            <CodeBlock
              title={selectedPath}
              language={snippet.language}
              code={snippet.code}
              actions={<IconButton size="sm" label={`Copy ${selectedPath}`} icon={<CopyIcon />} onClick={() => copy(snippet.code)} />}
            />
          ) : (
            <EmptyState
              icon={<FileIcon size={28} />}
              title={selectedPath ? "No preview for this file yet" : "Select a file"}
              description={
                selectedPath
                  ? "File contents appear here once AiDEX Core can read the workspace."
                  : "Choose a file to inspect it or ask about it in a chat."
              }
            />
          )}
        </Stack>
      </section>
    </Split>
  );
}
