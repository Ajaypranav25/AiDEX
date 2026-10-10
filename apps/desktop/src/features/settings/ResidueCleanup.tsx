import { useState } from "react";
import { SectionHeader } from "@aidex/ui/components";
import { TrashIcon } from "@aidex/ui/icons";
import { Button, Checkbox, Stack, Text } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { sampleResidue } from "./sample";

/** Files AiDEX left on this computer, with a running total of what is selected for removal. */
export function ResidueCleanup() {
  const notify = useToast();
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set(sampleResidue.map((item) => item.id)));

  const totalMb = sampleResidue.filter((item) => selected.has(item.id)).reduce((sum, item) => sum + item.sizeMb, 0);

  const toggle = (id: string, checked: boolean) =>
    setSelected((current) => {
      const next = new Set(current);
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });

  return (
    <section>
      <SectionHeader title="Host cleanup" meta={`${totalMb} MB selected`} />
      <Stack gap={5}>
        <Text tone="secondary">AiDEX keeps your work on the drive. These leftovers on this computer are safe to remove.</Text>
        <Stack gap={4} role="group" aria-label="Items to remove">
          {sampleResidue.map((item) => (
            <Checkbox
              key={item.id}
              label={`${item.label} · ${item.sizeMb} MB`}
              description={item.description}
              checked={selected.has(item.id)}
              onChange={(event) => toggle(item.id, event.target.checked)}
            />
          ))}
        </Stack>
        <Stack direction="row">
          <Button
            variant="outline"
            size="md"
            icon={<TrashIcon />}
            disabled={totalMb === 0}
            onClick={() => notify(`Preview: removing ${totalMb} MB from this computer will be available when AiDEX Core is connected.`)}
          >
            Clean up selected
          </Button>
        </Stack>
      </Stack>
    </section>
  );
}
