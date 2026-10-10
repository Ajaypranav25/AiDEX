import { Meter, PageHeader } from "@aidex/ui/components";
import { ChoiceCardGroup, Stack, Text } from "@aidex/ui/primitives";
import { applyPreset, driveById, estimateGb, formatGb, presetOptions, type Preset } from "../../lib/setup";
import { stepKicker } from "../../lib/steps";
import type { StepProps } from "../types";

/** Step 2: pick a preset that seeds the model, agent, and extension selections. */
export function ConfigurationStep({ state, update, titleRef }: StepProps) {
  const estimate = estimateGb(state);
  const drive = driveById(state.driveId);

  return (
    <>
      <PageHeader
        eyebrow={stepKicker(1)}
        title="Choose an installation type"
        description="Start with a preset. You can adjust models and agents in the next steps."
        titleRef={titleRef}
      />
      <ChoiceCardGroup
        label="Installation profile"
        variant="rows"
        options={presetOptions}
        value={state.preset}
        onValueChange={(preset: Preset) => update(applyPreset(state, preset))}
      />
      <Stack gap={3}>
        <Meter
          label="Estimated required space"
          value={estimate}
          max={drive.freeGb}
          valueText={`${formatGb(estimate)} of ${drive.freeGb} GB free (estimate)`}
        />
        <Text tone="muted">Storage estimates are illustrative and depend on the actual model files and platform binaries.</Text>
      </Stack>
    </>
  );
}
