import { SectionHeader } from "@aidex/ui/components";
import { Checkbox, Stack } from "@aidex/ui/primitives";
import { ComponentChecklist } from "../../components/ComponentChecklist";
import { SpaceSummary } from "../../components/SpaceSummary";
import { formatGb, RUNTIME, type ComponentGroupId, type SetupSelections } from "../../lib/setup";

type Props = {
  selections: SetupSelections;
  onToggle: (group: ComponentGroupId, id: string, on: boolean) => void;
};

/** Screen 31: local models, the required inference runtime, and acceleration backends with a live size meter. */
export function ModelsStep({ selections, onToggle }: Props) {
  return (
    <Stack gap={8}>
      <SpaceSummary selections={selections} />
      <ComponentChecklist
        title="Models"
        group="models"
        selections={selections}
        onToggle={onToggle}
        note="Quantized GGUF models. Choose at least one chat model; the embedding model powers workspace search."
      />
      <section>
        <Stack gap={4}>
          <SectionHeader title="Inference runtime" meta={`${formatGb(RUNTIME.sizeGb)} selected`} />
          <Checkbox
            label={RUNTIME.name}
            description={`${formatGb(RUNTIME.sizeGb)} · Required. Runs every local model offline from the drive.`}
            checked
            disabled
            readOnly
          />
        </Stack>
      </section>
      <ComponentChecklist
        title="Acceleration backends"
        group="backends"
        selections={selections}
        onToggle={onToggle}
        note="AiDEX picks the fastest available backend on each computer. Host details are sample values in this preview."
      />
    </Stack>
  );
}
