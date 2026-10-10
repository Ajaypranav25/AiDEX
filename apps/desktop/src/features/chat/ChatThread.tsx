import { ChatMessage, CodeBlock, InlineCode } from "@aidex/ui/components";
import { ChevronRightIcon } from "@aidex/ui/icons";
import { Button, Stack, Text } from "@aidex/ui/primitives";
import type { SampleExchange, Segment } from "./sample";

export type LocalMessage = { id: string; role: "user" | "assistant"; text: string; meta?: string };

type Props = {
  /** Seeded history for an existing session; absent for a brand-new chat. */
  exchange: SampleExchange | null;
  messages: readonly LocalMessage[];
  modelLabel: string;
  onContinueInAgent: () => void;
};

function Paragraph({ segments }: { segments: readonly Segment[] }) {
  return (
    <Text>
      {segments.map((segment, index) =>
        typeof segment === "string" ? segment : <InlineCode key={index}>{segment.code}</InlineCode>,
      )}
    </Text>
  );
}

function SampleTurn({ exchange, modelLabel, onContinueInAgent }: Omit<Props, "messages"> & { exchange: SampleExchange }) {
  return (
    <>
      <ChatMessage role="user" meta="Earlier">
        {exchange.prompt}
      </ChatMessage>
      <ChatMessage role="assistant" meta={modelLabel}>
        {exchange.intro.map((segments, index) => (
          <Paragraph key={index} segments={segments} />
        ))}
        <CodeBlock title={exchange.code.title} language={exchange.code.language} code={exchange.code.code} />
        <Paragraph segments={exchange.closing} />
        <Button variant="outline" size="md" trailing={<ChevronRightIcon />} onClick={onContinueInAgent}>
          Continue in Agent mode
        </Button>
      </ChatMessage>
    </>
  );
}

/** The conversation: sample history first, then messages sent in this preview. */
export function ChatThread({ exchange, messages, modelLabel, onContinueInAgent }: Props) {
  return (
    <Stack gap={8} role="log" aria-label="Conversation">
      {exchange ? <SampleTurn exchange={exchange} modelLabel={modelLabel} onContinueInAgent={onContinueInAgent} /> : null}
      {messages.map((message) => (
        <ChatMessage key={message.id} role={message.role} meta={message.meta}>
          {message.role === "user" ? message.text : <Text>{message.text}</Text>}
        </ChatMessage>
      ))}
    </Stack>
  );
}
