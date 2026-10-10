import { useEffect, useState } from "react";
import { DockLayout } from "@aidex/ui/desktop";
import { BotIcon, MoreHorizontalIcon } from "@aidex/ui/icons";
import { Container, Heading, IconButton, Stack, Text } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useToast } from "../../app/providers/toast";
import { modelOptions } from "../../lib/sample/models";
import { sessionById } from "../../lib/sample/sessions";
import { workspaceById } from "../../lib/sample/workspaces";
import { ChatComposer } from "./ChatComposer";
import { ChatThread, type LocalMessage } from "./ChatThread";
import { attachableFiles, exchangeFor } from "./sample";

let messageCount = 0;
const nextId = () => `message-${++messageCount}`;

const previewReply = (modelLabel: string, workspaceName: string): LocalMessage => ({
  id: nextId(),
  role: "assistant",
  text: `This preview doesn't call a model yet, so there is no real answer here. Once AiDEX Core is connected, ${modelLabel} will reply using ${workspaceName} as context.`,
  meta: `${modelLabel} · Preview`,
});

export function ChatPage() {
  const { route, navigate } = useNavigation();
  const { modelId, workspaceId, pendingPrompt, setPendingPrompt } = usePreferences();
  const notify = useToast();
  const modelLabel = modelOptions.find((option) => option.value === modelId)?.label ?? modelId;
  const workspace = workspaceById(workspaceId);

  // A prompt handed over from Home starts a new chat; it is read once, on arrival.
  const [arrivalPrompt] = useState(() => pendingPrompt);
  const isNewChat = !route.id && arrivalPrompt !== null;
  const session = isNewChat ? null : sessionById(route.id);
  const [messages, setMessages] = useState<readonly LocalMessage[]>(() =>
    arrivalPrompt ? [{ id: nextId(), role: "user", text: arrivalPrompt }, previewReply(modelLabel, workspace.name)] : [],
  );
  const [draft, setDraft] = useState("");
  const [attachments, setAttachments] = useState<readonly string[]>([]);

  useEffect(() => {
    if (pendingPrompt !== null) setPendingPrompt(null);
  }, [pendingPrompt, setPendingPrompt]);

  const submit = (text: string) => {
    const files = attachments.length ? `Attached: ${attachments.join(", ")}` : undefined;
    setMessages((current) => [
      ...current,
      { id: nextId(), role: "user", text: text.trim(), meta: files },
      previewReply(modelLabel, workspace.name),
    ]);
    setDraft("");
    setAttachments([]);
  };

  const attach = () => {
    const next = attachableFiles.find((path) => !attachments.includes(path));
    if (!next) {
      notify("Preview: choosing files from the workspace will be available when AiDEX Core is connected.");
      return;
    }
    setAttachments((current) => [...current, next]);
  };

  // Agent mode picks up the same session, or the typed draft as its task.
  const openAgent = (prompt: string | null) => {
    if (prompt) setPendingPrompt(prompt);
    navigate({ page: "agent", id: session?.id });
  };

  const dock = (
    <ChatComposer
      draft={draft}
      onDraftChange={setDraft}
      onSubmit={submit}
      attachments={attachments}
      onAttach={attach}
      onRemoveAttachment={(path) => setAttachments((current) => current.filter((item) => item !== path))}
      onSwitchToAgent={() => openAgent(draft.trim() || (isNewChat ? arrivalPrompt : null))}
    />
  );

  const continueInAgent = () => openAgent(isNewChat ? arrivalPrompt : null);

  return (
    <DockLayout width="prose" dock={dock}>
      {/* `verticallyCentered` supplies the block padding above the conversation; the dock pins the composer. */}
      <Container size="prose" verticallyCentered>
        <Stack gap={8}>
          <Stack direction="row" gap={3} align="start" justify="between">
            <Stack gap={1}>
              <Heading level={1} tabIndex={-1}>
                {session?.title ?? "New chat"}
              </Heading>
              <Text variant="label" tone="muted">
                Workspace in context: {workspace.name} · {workspace.branch}
              </Text>
            </Stack>
            <Stack direction="row" gap={1}>
              <IconButton label="Continue in Agent mode" icon={<BotIcon />} onClick={continueInAgent} />
              <IconButton
                label="More options"
                icon={<MoreHorizontalIcon />}
                onClick={() =>
                  notify("Preview: renaming, pinning, and exporting chats will be available when AiDEX Core is connected.")
                }
              />
            </Stack>
          </Stack>
          <ChatThread
            exchange={session ? exchangeFor(session.id, session.title) : null}
            messages={messages}
            modelLabel={session?.model ?? modelLabel}
            onContinueInAgent={continueInAgent}
          />
        </Stack>
      </Container>
    </DockLayout>
  );
}
