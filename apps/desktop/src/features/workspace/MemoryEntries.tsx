import { useRef, useState } from "react";
import { EmptyState, List, ListRow, SectionHeader } from "@aidex/ui/components";
import { LightbulbIcon, MemoryIcon, PencilIcon, PlusIcon, TrashIcon } from "@aidex/ui/icons";
import { Button, IconButton, Stack } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { sampleMemory, type MemoryEntry } from "./sample";

type Props = { workspaceName: string };

/** Facts and rules AiDEX keeps for this workspace across sessions. */
export function MemoryEntries({ workspaceName }: Props) {
  const notify = useToast();
  const [entries, setEntries] = useState<readonly MemoryEntry[]>(sampleMemory);
  // Deleting unmounts the row (and the last delete swaps the list for the empty state), so focus moves on explicitly.
  const deleteRefs = useRef(new Map<string, HTMLButtonElement>());
  const restoreRef = useRef<HTMLButtonElement>(null);
  const registerDelete = (id: string) => (element: HTMLButtonElement | null) => {
    if (element) deleteRefs.current.set(id, element);
    else deleteRefs.current.delete(id);
  };

  const remove = (entry: MemoryEntry) => {
    const index = entries.findIndex((candidate) => candidate.id === entry.id);
    const neighbourId = (entries[index + 1] ?? entries[index - 1])?.id;
    setEntries((current) => current.filter((candidate) => candidate.id !== entry.id));
    requestAnimationFrame(() => (neighbourId ? deleteRefs.current.get(neighbourId) : restoreRef.current)?.focus());
    notify(`Preview: “${entry.title}” is hidden for now. Deleting memory will be available when AiDEX Core is connected.`);
  };

  const restoreSamples = () => {
    setEntries(sampleMemory);
    const firstId = sampleMemory[0]?.id;
    requestAnimationFrame(() => (firstId ? deleteRefs.current.get(firstId) : undefined)?.focus());
  };

  return (
    <section aria-labelledby="memory-entries-title">
      <Stack gap={3}>
        <SectionHeader
          title={<span id="memory-entries-title">Persistent memory</span>}
          meta={`${entries.length} ${entries.length === 1 ? "entry" : "entries"}`}
          actions={
            <Button
              variant="outline"
              size="md"
              icon={<PlusIcon />}
              onClick={() => notify(`Preview: adding memory to ${workspaceName} will be available when AiDEX Core is connected.`)}
            >
              Add memory
            </Button>
          }
        />
        {entries.length > 0 ? (
          <List metaWidth="112px">
            {entries.map((entry) => (
              <ListRow
                key={entry.id}
                media={<LightbulbIcon />}
                title={entry.title}
                subtitle={entry.source}
                meta={[`Updated ${entry.updated.toLowerCase()}`]}
                trailing={
                  <Stack direction="row" gap={1}>
                    <IconButton
                      size="sm"
                      label={`Edit “${entry.title}”`}
                      icon={<PencilIcon />}
                      onClick={() => notify("Preview: editing memory will be available when AiDEX Core is connected.")}
                    />
                    <IconButton
                      ref={registerDelete(entry.id)}
                      size="sm"
                      label={`Delete “${entry.title}”`}
                      icon={<TrashIcon />}
                      onClick={() => remove(entry)}
                    />
                  </Stack>
                }
              />
            ))}
          </List>
        ) : (
          <EmptyState
            icon={<MemoryIcon size={28} />}
            title="No memory for this workspace"
            description="Facts you save from chats or agent rules appear here and are reused in later sessions."
            actions={
              <Button ref={restoreRef} variant="outline" size="md" onClick={restoreSamples}>
                Show sample entries again
              </Button>
            }
          />
        )}
      </Stack>
    </section>
  );
}
