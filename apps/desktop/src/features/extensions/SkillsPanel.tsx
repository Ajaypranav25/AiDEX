import { useRef, useState } from "react";
import { EmptyState, List, ListRow, SectionHeader } from "@aidex/ui/components";
import { LayersIcon, PencilIcon, SearchIcon } from "@aidex/ui/icons";
import { Badge, Button, IconBadge, SearchField, SegmentedControl, Split, Stack, Switch } from "@aidex/ui/primitives";
import { SkillEditor } from "./SkillEditor";
import { sampleSkills, skillCategories, type Skill } from "./sample";
import type { Toggles } from "./useToggles";

type StatusFilter = "all" | "enabled" | "disabled";

const filterOptions = [
  { value: "all", label: "All" },
  { value: "enabled", label: "Enabled" },
  { value: "disabled", label: "Disabled" },
] as const;

type Props = {
  toggles: Toggles;
  /** Skill to open in the editor on arrival, e.g. from the sidebar. */
  initialEditingId?: string;
};

function matches(skill: Skill, query: string, filter: StatusFilter, enabled: boolean): boolean {
  if (filter === "enabled" && !enabled) return false;
  if (filter === "disabled" && enabled) return false;
  const needle = query.trim().toLowerCase();
  return !needle || `${skill.name} ${skill.description}`.toLowerCase().includes(needle);
}

/** Skills tab: filterable skills grouped by category, each with an enable switch and a SKILL.md editor. */
export function SkillsPanel({ toggles, initialEditingId }: Props) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<StatusFilter>("all");
  const [editingId, setEditingId] = useState(sampleSkills.some((skill) => skill.id === initialEditingId) ? initialEditingId : undefined);
  const editing = sampleSkills.find((skill) => skill.id === editingId);
  const visible = sampleSkills.filter((skill) => matches(skill, query, filter, toggles.isOn(skill.id)));
  // Opening or closing the editor re-parents the list into or out of the Split, so focus is placed explicitly:
  // into the editor on Edit, and back to that skill's Edit button on close.
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const editButtonRefs = useRef(new Map<string, HTMLButtonElement>());
  const registerEditButton = (id: string) => (element: HTMLButtonElement | null) => {
    if (element) editButtonRefs.current.set(id, element);
    else editButtonRefs.current.delete(id);
  };

  const startEditing = (id: string) => {
    setEditingId(id);
    requestAnimationFrame(() => editorRef.current?.focus());
  };

  const closeEditor = () => {
    const closedId = editingId;
    setEditingId(undefined);
    requestAnimationFrame(() => (closedId ? editButtonRefs.current.get(closedId) : undefined)?.focus());
  };

  const list =
    visible.length === 0 ? (
      <EmptyState icon={<SearchIcon />} title="No skills match" description="Try another search or status filter." />
    ) : (
      <Stack gap={8}>
        {skillCategories.map((category) => {
          const skills = visible.filter((skill) => skill.category === category);
          if (skills.length === 0) return null;
          return (
            <section key={category} aria-label={category}>
              <SectionHeader title={category} meta={`${skills.length} skills`} />
              <List>
                {skills.map((skill) => (
                  <ListRow
                    key={skill.id}
                    media={<IconBadge size="md" icon={<LayersIcon />} />}
                    title={skill.name}
                    subtitle={skill.description}
                    meta={[<Badge key="category" tone="neutral">{skill.category}</Badge>]}
                    selected={skill.id === editingId}
                    trailing={
                      <Stack direction="row" gap={3} align="center">
                        <Switch
                          label={`Enable ${skill.name}`}
                          checked={toggles.isOn(skill.id)}
                          onCheckedChange={(on) => toggles.set(skill.id, on)}
                        />
                        <Button
                          ref={registerEditButton(skill.id)}
                          variant="ghost"
                          icon={<PencilIcon />}
                          aria-label={`Edit ${skill.name}`}
                          aria-pressed={skill.id === editingId}
                          onClick={() => startEditing(skill.id)}
                        >
                          Edit
                        </Button>
                      </Stack>
                    }
                  />
                ))}
              </List>
            </section>
          );
        })}
      </Stack>
    );

  return (
    <Stack gap={6}>
      <Stack direction="row" gap={3} align="center" wrap>
        <SearchField label="Search skills" placeholder="Search skills..." value={query} onChange={(event) => setQuery(event.target.value)} />
        <SegmentedControl size="sm" label="Skill status" options={filterOptions} value={filter} onValueChange={setFilter} />
      </Stack>
      {editing ? (
        <Split divided emphasis="end">
          {list}
          <SkillEditor key={editing.id} skill={editing} onClose={closeEditor} textAreaRef={editorRef} />
        </Split>
      ) : (
        list
      )}
    </Stack>
  );
}
