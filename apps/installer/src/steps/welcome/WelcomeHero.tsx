import type { Ref } from "react";
import { Brand, MediaItem } from "@aidex/ui/components";
import { BotIcon, LayersIcon, LockIcon } from "@aidex/ui/icons";
import { Grid, Heading, Stack, Text } from "@aidex/ui/primitives";

type Props = { titleRef: Ref<HTMLHeadingElement> };

const highlights = [
  { id: "models", Icon: LayersIcon, title: "Models", description: "Local models and the runtime travel with you." },
  { id: "agents", Icon: BotIcon, title: "Agents", description: "AiDEX Native plus the adapters you choose." },
  { id: "data", Icon: LockIcon, title: "Your data", description: "Workspaces live in an encrypted vault." },
] as const;

/** Welcome copy for screen 29: what AiDEX puts on a portable drive. */
export function WelcomeHero({ titleRef }: Props) {
  return (
    <Stack gap={8}>
      <Stack gap={6} align="start">
        <Brand size="xl" />
        <Stack gap={3}>
          <Heading ref={titleRef} tabIndex={-1} level={1} size="display">
            Take your AI workspace with you.
          </Heading>
          <Text variant="lead" tone="secondary">
            AiDEX Setup prepares a portable drive with your models, agents, and workspace so they work on any computer you plug into.
          </Text>
        </Stack>
      </Stack>
      <Grid columns={3} gap={4}>
        {highlights.map(({ id, Icon, title, description }) => (
          <MediaItem key={id} media={<Icon size={22} />} title={title} description={description} />
        ))}
      </Grid>
    </Stack>
  );
}
