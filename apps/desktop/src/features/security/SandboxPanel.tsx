import { List, ListRow, SectionHeader } from "@aidex/ui/components";
import { AlertTriangleIcon, LaptopIcon, MonitorIcon, ServerIcon } from "@aidex/ui/icons";
import { Badge, ChoiceCardGroup, Stack, StatusIndicator, Text, type ChoiceCardOption } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import { PolicyRules } from "./PolicyRules";
import { sandboxBackends, type IsolationLevel } from "./sample";
import { useHostPlatform } from "./useHostPlatform";

type Props = { isolation: IsolationLevel; onIsolationChange: (level: IsolationLevel) => void };

const isolationOptions: readonly ChoiceCardOption<IsolationLevel>[] = [
  {
    value: "off",
    title: "Off",
    description: "Commands run directly as your user account, with no OS restrictions. Use only for trusted, one-off work.",
    meta: <Badge tone="neutral" icon={<AlertTriangleIcon size={14} />}>Not recommended</Badge>,
  },
  {
    value: "standard",
    title: "Standard",
    description: "Every sensitive action goes through the broker, and commands run with native OS limits on files, resources, and network.",
    meta: <Badge tone="accent">Recommended</Badge>,
  },
  {
    value: "strict",
    title: "Strict",
    description: "Agent work runs in an ephemeral isolated environment, with file changes held in an overlay for you to review.",
  },
];

const backendIcons = { windows: MonitorIcon, linux: ServerIcon, macos: LaptopIcon } as const;

const toastByLevel: Record<IsolationLevel, string> = {
  off: "Preview: isolation is off here. Agent commands would run as your user without OS restrictions.",
  standard: "Preview: Standard isolation selected. AiDEX Core will apply it to new agent processes once connected.",
  strict: "Preview: Strict isolation selected. AiDEX Core will apply it to new agent processes once connected.",
};

/** Sandbox and policies: isolation level, per-platform backends, and filesystem and network rules. */
export function SandboxPanel({ isolation, onIsolationChange }: Props) {
  const notify = useToast();
  const host = useHostPlatform();

  const changeIsolation = (level: IsolationLevel) => {
    onIsolationChange(level);
    notify(toastByLevel[level], level === "off" ? "danger" : "info");
  };

  return (
    <Stack gap={10}>
      <section>
        <Stack gap={4}>
          <SectionHeader title="Isolation level" />
          <ChoiceCardGroup
            label="Isolation level"
            labelVisibility="hidden"
            columns={3}
            options={isolationOptions}
            value={isolation}
            onValueChange={changeIsolation}
          />
        </Stack>
      </section>

      <section>
        <Stack gap={4}>
          <SectionHeader title="Platform backends" />
          <List>
            {sandboxBackends.map((backend) => {
              const Icon = backendIcons[backend.id];
              const isHost = backend.id === host;
              return (
                <ListRow
                  key={backend.id}
                  media={<Icon />}
                  title={`${backend.platform}: ${backend.mechanism}`}
                  subtitle={backend.detail}
                  trailing={
                    <StatusIndicator tone={isHost ? "success" : "neutral"} label={isHost ? "This computer" : "Other platform"} />
                  }
                />
              );
            })}
          </List>
          <Text tone="secondary">
            Isolation fails closed. If this computer can't enforce the selected level, the broker blocks the action instead of running
            it with weaker isolation.
          </Text>
        </Stack>
      </section>

      <PolicyRules />
    </Stack>
  );
}
