import { Meter, PageHeader } from "@aidex/ui/components";
import { ChoiceCardGroup, Stack, Text } from "@aidex/ui/primitives";
import { applyPreset, estimateGb, formatGb, freeGb, presetOptions, type Preset } from "../../lib/setup";
import { stepKicker } from "../../lib/steps";
import type { StepProps } from "../types";

/** Step 2: pick a preset that seeds the model, agent, and extension selections. */
export function ConfigurationStep({ state, update, titleRef, volume }: StepProps) {
  const estimate = estimateGb(state);
  const free = freeGb(volume);

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
        {free === null ? (
          <Text tone="secondary">Estimated size {formatGb(estimate)}. Choose a drive to compare it with real free space.</Text>
        ) : (
          <Meter
            label="Estimated required space"
            value={estimate}
            max={free}
            valueText={`${formatGb(estimate)} of ${formatGb(free)} free on ${volume?.root ?? "the drive"} (estimate)`}
          />
        )}
        <Text tone="muted">Free space is read from the drive; component sizes are planning estimates until the portable bundle ships.</Text>
      </Stack>
    </>
  );
}
