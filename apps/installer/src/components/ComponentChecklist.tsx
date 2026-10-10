import type { ReactNode } from "react";
import { SectionHeader } from "@aidex/ui/components";
import { Checkbox, Stack, Text } from "@aidex/ui/primitives";
import { catalog, formatGb, groupSizeGb, type ComponentGroupId, type SetupSelections } from "../lib/setup";

type Props = {
  title: string;
  group: ComponentGroupId;
  selections: SetupSelections;
  onToggle: (group: ComponentGroupId, id: string, on: boolean) => void;
  /** Context under the heading, e.g. why these items are optional. */
  note?: ReactNode;
  level?: 2 | 3;
};

function detail(sizeGb: number, description: string, required?: boolean, recommended?: boolean): string {
  const tags = [formatGb(sizeGb), required ? "Required" : null, recommended ? "Recommended" : null].filter(Boolean);
  return `${tags.join(" · ")}. ${description}`;
}

/** One catalog group as a checkbox list with its selected size; required items stay checked and disabled. */
export function ComponentChecklist({ title, group, selections, onToggle, note, level = 2 }: Props) {
  const items = catalog[group];
  const chosen = selections.components[group];
  return (
    <section>
      <Stack gap={4}>
        <Stack gap={2}>
          <SectionHeader title={title} level={level} meta={`${formatGb(groupSizeGb(selections.components, group))} selected`} />
          {note ? <Text tone="muted">{note}</Text> : null}
        </Stack>
        <Stack gap={4}>
          {items.map((item) => (
            <Checkbox
              key={item.id}
              label={item.name}
              description={detail(item.sizeGb, item.description, item.required, item.recommended)}
              checked={item.required || chosen.includes(item.id)}
              disabled={item.required}
              onChange={(event) => onToggle(group, item.id, event.currentTarget.checked)}
            />
          ))}
        </Stack>
      </Stack>
    </section>
  );
}
