import type { Ref } from "react";
import { PageHeader } from "@aidex/ui/components";
import { FileIcon } from "@aidex/ui/icons";
import { CheckboxGroup, Grid, Select, Stack, Text, TextField } from "@aidex/ui/primitives";
import { backendOptions, modelOptions, type Backend, type ModelId } from "../../lib/setup";
import { stepKicker } from "../../lib/steps";
import type { StepProps } from "../types";

type Props = StepProps & {
  customPathError?: string;
  customPathRef: Ref<HTMLInputElement>;
};

const runtimeOptions = [
  { value: "llama.cpp", title: "llama.cpp", description: "Portable inference engine", meta: "Included", disabled: true },
] as const;

/** Step 3: choose models to carry on the drive and the inference backend. */
export function ModelsStep({ state, update, titleRef, customPathError, customPathRef }: Props) {
  return (
    <>
      <PageHeader
        eyebrow={stepKicker(2)}
        title="Local models & runtime"
        description="Choose models to store on your drive. Local inference can work without cloud APIs."
        titleRef={titleRef}
      />
      <Grid columns={2} gap={8}>
        <Stack gap={4}>
          <CheckboxGroup
            label="Models"
            options={modelOptions}
            value={state.models}
            // Editing components by hand turns the setup into a custom one.
            onValueChange={(models) => update({ models: models as ModelId[], preset: "custom" })}
          />
          {state.models.includes("custom") ? (
            <TextField
              ref={customPathRef}
              label="GGUF file path"
              placeholder="File location"
              leadingIcon={<FileIcon />}
              size="md"
              value={state.customModelPath}
              error={customPathError}
              onChange={(event) => update({ customModelPath: event.target.value })}
            />
          ) : null}
        </Stack>
        <Stack gap={5}>
          <CheckboxGroup label="Inference runtime" options={runtimeOptions} value={["llama.cpp"]} onValueChange={() => {}} />
          <Select
            label="Execution backend"
            options={backendOptions}
            value={state.backend}
            hint="The runtime detects the host's supported acceleration backends when AiDEX starts."
            onChange={(event) => update({ backend: event.target.value as Backend })}
          />
        </Stack>
      </Grid>
      <Text tone="muted">Models shown are sample package choices. This preview does not download or inspect files.</Text>
    </>
  );
}
