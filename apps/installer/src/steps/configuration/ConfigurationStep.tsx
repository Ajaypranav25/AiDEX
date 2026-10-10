import { SectionHeader } from "@aidex/ui/components";
import { CodeIcon, SlidersIcon, ZapIcon } from "@aidex/ui/icons";
import { Badge, Checkbox, ChoiceCardGroup, Stack, Text, type ChoiceCardOption } from "@aidex/ui/primitives";
import { SpaceSummary } from "../../components/SpaceSummary";
import {
  customParts,
  estimateGb,
  formatGb,
  partEnabled,
  profilePresets,
  type ProfileId,
  type SetupSelections,
} from "../../lib/setup";

type Props = {
  selections: SetupSelections;
  onProfileChange: (profile: ProfileId) => void;
  onPartChange: (partId: string, on: boolean) => void;
};

function profileOptions(selections: SetupSelections): ChoiceCardOption<ProfileId>[] {
  return [
    {
      value: "lite",
      title: "Lite",
      description: "AiDEX core with one small model. Best for drives under 256 GB and quick setup.",
      icon: <ZapIcon />,
      meta: <Badge tone="neutral">≈ {formatGb(estimateGb(profilePresets.lite))}</Badge>,
    },
    {
      value: "developer",
      title: "Developer",
      description: "All models, agent adapters, skill packs, MCP servers, and Node.js, Python, and Rust toolchains.",
      icon: <CodeIcon />,
      meta: <Badge tone="neutral">≈ {formatGb(estimateGb(profilePresets.developer))}</Badge>,
    },
    {
      value: "custom",
      title: "Custom",
      description: "Choose components yourself, then refine models and agents in the next steps.",
      icon: <SlidersIcon />,
      meta: <Badge tone="neutral">≈ {formatGb(estimateGb(selections.components))}</Badge>,
    },
  ];
}

/** Screen 30: installation profile with a size estimate; Custom reveals component bundles. */
export function ConfigurationStep({ selections, onProfileChange, onPartChange }: Props) {
  return (
    <Stack gap={8}>
      <ChoiceCardGroup
        label="Installation profile"
        options={profileOptions(selections)}
        value={selections.profile}
        onValueChange={onProfileChange}
      />
      {selections.profile === "custom" ? (
        <section>
          <Stack gap={4}>
            <Stack gap={2}>
              <SectionHeader title="Components" />
              <Text tone="muted">AiDEX core, the llama.cpp runtime, and the search embedding model are always included.</Text>
            </Stack>
            <Stack gap={4}>
              {customParts.map((part) => (
                <Checkbox
                  key={part.id}
                  label={part.title}
                  description={part.description}
                  checked={partEnabled(selections, part.id)}
                  onChange={(event) => onPartChange(part.id, event.currentTarget.checked)}
                />
              ))}
            </Stack>
          </Stack>
        </section>
      ) : null}
      <section>
        <Stack gap={4}>
          <SectionHeader title="Estimated size" />
          <SpaceSummary selections={selections} />
        </Stack>
      </section>
    </Stack>
  );
}
