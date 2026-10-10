import { useState, type FormEvent, type Ref } from "react";
import { SectionHeader } from "@aidex/ui/components";
import { Button, Stack, TextArea } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import type { Skill } from "./sample";

type Props = {
  skill: Skill;
  onClose: () => void;
  /** Focus target when the editor opens from an Edit button. */
  textAreaRef?: Ref<HTMLTextAreaElement>;
};

const EDITOR_ROWS = 16;

/** SKILL.md editor for one skill; remount it (via `key`) to start a fresh draft. */
export function SkillEditor({ skill, onClose, textAreaRef }: Props) {
  const notify = useToast();
  const [draft, setDraft] = useState(skill.definition);
  const headingId = `skill-editor-${skill.id}`;

  const save = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    notify(`Preview: saving ${skill.name} will be available when AiDEX Core is connected.`);
  };

  return (
    <section aria-labelledby={headingId}>
      <form onSubmit={save}>
        <Stack gap={4}>
          <SectionHeader title={<span id={headingId}>Edit {skill.name}</span>} meta="SKILL.md" />
          <TextArea
            ref={textAreaRef}
            label={`${skill.name} SKILL.md`}
            labelVisibility="hidden"
            monospace
            rows={EDITOR_ROWS}
            spellCheck={false}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            hint="Front matter names the skill; the body is the instructions agents follow."
          />
          <Stack direction="row" gap={2} justify="end">
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={draft === skill.definition}>
              Save
            </Button>
          </Stack>
        </Stack>
      </form>
    </section>
  );
}
