import { Stack } from "@aidex/ui/primitives";
import { ComponentChecklist } from "../../components/ComponentChecklist";
import { SpaceSummary } from "../../components/SpaceSummary";
import type { ComponentGroupId, SetupSelections } from "../../lib/setup";

type Props = {
  selections: SetupSelections;
  onToggle: (group: ComponentGroupId, id: string, on: boolean) => void;
};

/** Screen 32: agents, skill packs, MCP servers, and toolchains, each with sizes. */
export function AgentsStep({ selections, onToggle }: Props) {
  return (
    <Stack gap={8}>
      <SpaceSummary selections={selections} />
      <ComponentChecklist
        title="Agents"
        group="agents"
        selections={selections}
        onToggle={onToggle}
        note="External agents are optional adapters. AiDEX does not include accounts or API keys; you sign in after setup."
      />
      <ComponentChecklist title="Skill packs" group="skills" selections={selections} onToggle={onToggle} />
      <ComponentChecklist
        title="MCP servers"
        group="mcp"
        selections={selections}
        onToggle={onToggle}
        note="Servers run only inside workspaces you approve."
      />
      <ComponentChecklist
        title="Toolchains"
        group="toolchains"
        selections={selections}
        onToggle={onToggle}
        note="Portable toolchains run from the drive without changing the host computer."
      />
    </Stack>
  );
}
