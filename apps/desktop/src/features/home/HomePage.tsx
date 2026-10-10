import { useRef, useState } from "react";
import { Composer } from "@aidex/ui/components";
import { CodeIcon, FileTextIcon, LightbulbIcon, MessageSquareIcon } from "@aidex/ui/icons";
import { Button, Container, Heading, SegmentedControl, Stack, Text } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { SystemOverview } from "./SystemOverview";

type Mode = "chat" | "agent";

type Props = { autoFocus?: boolean };

const modeOptions = [
  { value: "chat", label: "Chat" },
  { value: "agent", label: "Agent" },
] as const;

const placeholderByMode: Record<Mode, string> = {
  chat: "Ask anything, start a task, or work on your codebase...",
  agent: "Describe a task for an agent to carry out...",
};

const suggestions = [
  { label: "Summarize notes", Icon: FileTextIcon, prompt: "Summarize these notes: " },
  { label: "Fix a bug", Icon: CodeIcon, prompt: "Help me fix a bug: " },
  { label: "Plan feature", Icon: LightbulbIcon, prompt: "Help me plan a feature: " },
  { label: "Review code", Icon: MessageSquareIcon, prompt: "Review this code: " },
] as const;

export function HomePage({ autoFocus = false }: Props) {
  const [mode, setMode] = useState<Mode>("chat");
  const [draft, setDraft] = useState("");
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const { navigate } = useNavigation();
  const { setPendingPrompt } = usePreferences();

  // The chat or agent screen picks the prompt up on arrival.
  const submit = (prompt: string) => {
    setPendingPrompt(prompt.trim());
    navigate({ page: mode === "agent" ? "agent" : "chat" });
  };

  // Frames whatever is already typed instead of discarding it.
  const applySuggestion = (prompt: string) => {
    setDraft((current) => (current.trim() ? prompt + current.trimStart() : prompt));
    composerRef.current?.focus();
  };

  return (
    <Container size="md" verticallyCentered>
      <Stack gap={11}>
        <Stack direction="row" justify="center">
          <SegmentedControl label="Conversation mode" options={modeOptions} value={mode} onValueChange={setMode} />
        </Stack>
        <Stack gap={10}>
          <Stack gap={3}>
            <Heading level={1} size="display" align="center">
              Welcome to AiDEX
            </Heading>
            <Text variant="lead" tone="secondary" align="center">
              Your AI workspace for deeper thinking, faster building, and greater outcomes.
            </Text>
          </Stack>
          <Stack gap={7}>
            <Composer
              label="Message"
              placeholder={placeholderByMode[mode]}
              value={draft}
              onValueChange={setDraft}
              onSubmit={submit}
              textareaRef={composerRef}
              autoFocus={autoFocus}
            />
            <Stack direction="row" gap={4} justify="center" wrap role="group" aria-label="Suggestions">
              {suggestions.map(({ label, Icon, prompt }) => (
                <Button key={label} variant="outline" size="lg" icon={<Icon />} onClick={() => applySuggestion(prompt)}>
                  {label}
                </Button>
              ))}
            </Stack>
          </Stack>
        </Stack>
        <SystemOverview />
      </Stack>
    </Container>
  );
}
