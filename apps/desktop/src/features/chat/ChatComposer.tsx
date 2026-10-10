import { Composer } from "@aidex/ui/components";
import { FileIcon, XIcon } from "@aidex/ui/icons";
import { Badge, IconButton, SegmentedControl, Select, Stack, Text } from "@aidex/ui/primitives";
import { usePreferences } from "../../app/providers/preferences";
import { modelOptions } from "../../lib/sample/models";

type Props = {
  draft: string;
  onDraftChange: (value: string) => void;
  onSubmit: (value: string) => void;
  attachments: readonly string[];
  onAttach: () => void;
  onRemoveAttachment: (path: string) => void;
  /** Switching to Agent hands the draft to the agent screen. */
  onSwitchToAgent: () => void;
};

const modeOptions = [
  { value: "chat", label: "Chat" },
  { value: "agent", label: "Agent" },
] as const;

function AttachmentChips({ paths, onRemove }: { paths: readonly string[]; onRemove: (path: string) => void }) {
  return (
    <Stack direction="row" gap={2} wrap role="list" aria-label="Attached files">
      {paths.map((path) => (
        <Stack key={path} direction="row" gap={1} align="center" role="listitem">
          <Badge tone="neutral" icon={<FileIcon />}>
            {path}
          </Badge>
          <IconButton size="sm" label={`Remove ${path}`} icon={<XIcon />} onClick={() => onRemove(path)} />
        </Stack>
      ))}
    </Stack>
  );
}

/** Docked chat composer: attachments, Chat/Agent switch, model picker, and the preview note. */
export function ChatComposer({
  draft,
  onDraftChange,
  onSubmit,
  attachments,
  onAttach,
  onRemoveAttachment,
  onSwitchToAgent,
}: Props) {
  const { modelId, setModelId } = usePreferences();

  return (
    <Stack gap={2}>
      <Composer
        label="Message"
        placeholder="Ask AiDEX anything..."
        value={draft}
        onValueChange={onDraftChange}
        onSubmit={onSubmit}
        onAttach={onAttach}
        attachments={attachments.length ? <AttachmentChips paths={attachments} onRemove={onRemoveAttachment} /> : undefined}
        toolbarStart={
          <SegmentedControl
            label="Conversation mode"
            size="sm"
            options={modeOptions}
            value="chat"
            onValueChange={(mode) => {
              if (mode === "agent") onSwitchToAgent();
            }}
          />
        }
        toolbarEnd={
          <Select
            label="Model"
            labelVisibility="hidden"
            variant="ghost"
            options={modelOptions}
            value={modelId}
            onChange={(event) => setModelId(event.target.value)}
          />
        }
      />
      <Text variant="label" tone="muted" align="center">
        Preview interface — no model calls are sent.
      </Text>
    </Stack>
  );
}
