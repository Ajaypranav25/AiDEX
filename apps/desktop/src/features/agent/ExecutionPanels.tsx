import {
  DataTable,
  InlineCode,
  List,
  ListRow,
  SectionHeader,
  StepList,
  TaskGraph,
  Terminal,
  type DataTableColumn,
} from "@aidex/ui/components";
import { BotIcon, CodeIcon, FileTextIcon, FlaskIcon, SearchIcon } from "@aidex/ui/icons";
import { IconBadge, Split, Stack, Text } from "@aidex/ui/primitives";
import type { ReactNode } from "react";
import { planSteps, stepsAt, statusText, subagents, terminalLines, toolCalls, type ToolCall } from "./sample";

/** The execution view shows the run as it stands after its first step. */
const SNAPSHOT_COMPLETED = 1;

const toolColumns: readonly DataTableColumn<ToolCall>[] = [
  { key: "tool", header: "Tool", rowHeader: true, render: (call) => <InlineCode>{call.tool}</InlineCode> },
  { key: "target", header: "Target" },
  { key: "status", header: "Status" },
  { key: "duration", header: "Duration", align: "end" },
];

export function ExecutionPanel() {
  return (
    <Split divided emphasis="end">
      <section>
        <Stack gap={5}>
          <SectionHeader title="Task progress" meta={`${SNAPSHOT_COMPLETED} / ${planSteps.length}`} />
          <StepList steps={stepsAt(SNAPSHOT_COMPLETED)} />
        </Stack>
      </section>
      <section>
        <Stack gap={5}>
          <SectionHeader title="Tool calls" meta={`${toolCalls.length} calls`} />
          <DataTable caption="Tool calls in this run" hideCaption columns={toolColumns} rows={toolCalls} rowKey={(call) => call.id} />
          <Terminal title="shell.exec · pnpm test auth" lines={terminalLines} />
          <Text variant="label" tone="muted">
            Actions pass through the AiDEX execution broker and policy engine.
          </Text>
        </Stack>
      </section>
    </Split>
  );
}

const subagentIcons: Record<string, ReactNode> = {
  analyze: <SearchIcon />,
  implement: <CodeIcon />,
  verify: <FlaskIcon />,
  docs: <FileTextIcon />,
};

export function GraphPanel() {
  return (
    <Stack gap={8}>
      <Stack gap={4}>
        <TaskGraph
          root={{ id: "root", label: "AiDEX agent", icon: <BotIcon />, status: "running", detail: "Orchestrating 4 workers" }}
          nodes={subagents.map(({ id, label, status, detail }) => ({ id, label, status, detail, icon: subagentIcons[id] }))}
        />
        <Text variant="label" tone="muted" align="center">
          Each worker runs in its own restricted context.
        </Text>
      </Stack>
      <section>
        <Stack gap={4}>
          <SectionHeader title="Subagents" meta={`${subagents.length} workers`} />
          <List metaWidth="160px">
            {subagents.map(({ id, label, detail, model, tools, status }) => (
              <ListRow
                key={id}
                media={<IconBadge icon={subagentIcons[id]} />}
                title={label}
                subtitle={detail}
                meta={[model, tools, statusText[status]]}
              />
            ))}
          </List>
        </Stack>
      </section>
    </Stack>
  );
}
