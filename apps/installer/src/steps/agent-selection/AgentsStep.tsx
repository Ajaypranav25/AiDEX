import { Notice, PageHeader } from "@aidex/ui/components";
import { CheckboxGroup, Grid } from "@aidex/ui/primitives";
import { agentOptions, extensionOptions, type AgentId, type ExtensionId } from "../../lib/setup";
import { stepKicker } from "../../lib/steps";
import type { StepProps } from "../types";

const NATIVE = "native";
const runtimeOptions = [
  { value: NATIVE, title: "AiDEX Native", description: "Chat and agent harness", meta: "Included", disabled: true },
  ...agentOptions,
];

/** Step 4: choose external agent runtimes and extensions to carry alongside AiDEX Native. */
export function AgentsStep({ state, update, titleRef }: StepProps) {
  return (
    <>
      <PageHeader
        eyebrow={stepKicker(3)}
        title="Agents & extensions"
        description="Choose the portable coding agents and tools that AiDEX should carry."
        titleRef={titleRef}
      />
      <Grid columns={2} gap={8}>
        <CheckboxGroup
          label="Agent runtimes"
          options={runtimeOptions}
          value={[NATIVE, ...state.agents]}
          onValueChange={(values) =>
            update({ agents: values.filter((value) => value !== NATIVE) as AgentId[], preset: "custom" })
          }
        />
        <CheckboxGroup
          label="Extensions"
          options={extensionOptions}
          value={state.extensions}
          onValueChange={(extensions) => update({ extensions: extensions as ExtensionId[], preset: "custom" })}
        />
      </Grid>
      <Notice>External CLIs may require sign-in on the destination host. No credentials are bundled.</Notice>
    </>
  );
}
