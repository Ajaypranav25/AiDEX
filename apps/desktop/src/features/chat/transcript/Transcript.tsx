import { useMemo, type ReactNode } from "react";
import { Stack } from "@aidex/ui/primitives";
import type { Message } from "@aidex/client/dto/sessions";
import { ArtifactResultRow, PlanProposalRow } from "./ArtifactRows";
import { MessageContent } from "./MessageContent";
import { McpRow, SkillLoadRow, WebFetchRow, WebSearchGroupRow, WebSearchRow } from "./ToolRows";
import {
  buildTranscript,
  type ToolEntry,
  type TranscriptItem,
  type TranscriptOptions,
} from "./transcriptModel";

function ToolEntryView({ entry }: { entry: ToolEntry }) {
  switch (entry.kind) {
    case "web_search":
      return <WebSearchRow entry={entry} />;
    case "web_fetch":
      return <WebFetchRow entry={entry} />;
    case "mcp":
      return <McpRow entry={entry} />;
    case "skill_load":
      return <SkillLoadRow entry={entry} />;
    case "artifact_create":
    case "artifact_update":
      return <ArtifactResultRow entry={entry} />;
    case "plan_propose":
      return <PlanProposalRow entry={entry} />;
  }
}

/** Work-log rows sit close together; messages and cards keep the transcript's spacing. */
function isWorkLogRow(item: TranscriptItem): boolean {
  if (item.type === "web-searches") return true;
  return item.type === "tool" && ["web_search", "web_fetch", "mcp", "skill_load"].includes(item.entry.kind);
}

function renderItem(item: TranscriptItem): ReactNode {
  if (item.type === "message")
    return <MessageContent key={item.key} message={item.message} hiddenCalls={item.hiddenCalls} />;
  if (item.type === "tool") return <ToolEntryView key={item.key} entry={item.entry} />;
  return <WebSearchGroupRow key={item.key} entries={item.entries} />;
}

/**
 * Messages as a transcript: chat bubbles and Markdown replies, with web, MCP, skill, artifact, and
 * plan tool calls as work-log rows and cards. Other tools keep their arguments and output inline.
 * Needs a `TranscriptProvider`.
 */
export function Transcript({ messages, options }: { messages: readonly Message[]; options: TranscriptOptions }) {
  const items = useMemo(
    () => buildTranscript(messages, options),
    [messages, options.running, options.complete],
  );
  const blocks: ReactNode[] = [];
  let rows: TranscriptItem[] = [];
  const flush = () => {
    if (rows.length)
      blocks.push(
        <Stack key={`rows-${rows[0].key}`} gap={1}>
          {rows.map(renderItem)}
        </Stack>,
      );
    rows = [];
  };
  for (const item of items) {
    if (isWorkLogRow(item)) rows.push(item);
    else {
      flush();
      blocks.push(renderItem(item));
    }
  }
  flush();
  return <>{blocks}</>;
}
