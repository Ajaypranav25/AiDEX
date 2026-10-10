import { useEffect, useRef, useState } from "react";
import { Composer, PageHeader } from "@aidex/ui/components";
import { DockLayout, PageFrame } from "@aidex/ui/desktop";
import { ChevronRightIcon, FolderIcon, GitBranchIcon, PauseIcon, PlayIcon, ShieldCheckIcon, StopIcon } from "@aidex/ui/icons";
import { Badge, Button, Split, Stack, StatusIndicator, type StatusIndicatorProps } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useToast } from "../../app/providers/toast";
import { sampleSessions } from "../../lib/sample/sessions";
import { workspaceById } from "../../lib/sample/workspaces";
import { ActivitySection, PermissionsSection, PlanSection } from "./AgentPanels";
import { DEFAULT_TASK } from "./sample";
import { useDemoRun, type RunStatus } from "./useDemoRun";
import { useTaskName } from "./useTaskName";

const SAMPLE_RUN_NOTE = "Sample run: the plan and activity below are illustrative until AiDEX Core runs agents.";

const STATUS: Record<RunStatus, { label: string; tone: StatusIndicatorProps["tone"] }> = {
  running: { label: "Running", tone: "success" },
  paused: { label: "Paused", tone: "warning" },
  stopped: { label: "Stopped", tone: "neutral" },
  complete: { label: "Complete", tone: "success" },
};

export function AgentPage() {
  const { route, navigate } = useNavigation();
  const { workspaceId, pendingPrompt, setPendingPrompt } = usePreferences();
  const notify = useToast();
  const taskName = useTaskName();
  const run = useDemoRun();
  const [draft, setDraft] = useState("");
  // Stopping disables Pause and Stop, and the last Advance disables itself, so focus moves on to what changed.
  const logRef = useRef<HTMLOListElement>(null);
  const reviewRef = useRef<HTMLButtonElement>(null);

  // The handed-over prompt became the task name; clear it so later screens start fresh.
  useEffect(() => {
    if (pendingPrompt !== null) setPendingPrompt(null);
  }, [pendingPrompt, setPendingPrompt]);

  const session = sampleSessions.find((candidate) => candidate.id === route.id);
  const workspace = workspaceById(session?.workspaceId ?? workspaceId);
  const status = STATUS[run.status];
  const finished = run.status === "stopped" || run.status === "complete";

  const stop = () => {
    run.stop();
    notify("Preview: this sample run stopped. Live runs will stop through AiDEX Core.");
    requestAnimationFrame(() => logRef.current?.focus());
  };

  const advance = () => {
    const lastStep = run.completed + 1 >= run.stepCount;
    run.advance();
    if (lastStep) requestAnimationFrame(() => reviewRef.current?.focus());
  };

  const submitInstruction = (text: string) => {
    run.instruct(text.trim());
    setDraft("");
  };

  const meta = (
    <Stack direction="row" gap={4} align="center" wrap>
      <StatusIndicator label={status.label} tone={status.tone} />
      <Badge tone="neutral" icon={<FolderIcon />} title="Workspace">
        {workspace.name}
      </Badge>
      <Badge tone="neutral" icon={<GitBranchIcon />} title="Branch">
        {workspace.branch}
      </Badge>
      <Badge tone="accent" icon={<ShieldCheckIcon />}>
        Asks before edits
      </Badge>
      <Button variant="ghost" size="md" trailing={<ChevronRightIcon />} onClick={() => navigate({ page: "execution" })}>
        View execution
      </Button>
    </Stack>
  );

  const actions = (
    <>
      <Button
        variant="outline"
        size="md"
        icon={run.status === "paused" ? <PlayIcon /> : <PauseIcon />}
        disabled={finished}
        onClick={run.togglePause}
      >
        {run.status === "paused" ? "Resume" : "Pause"}
      </Button>
      <Button variant="outline" size="md" icon={<StopIcon />} disabled={finished} onClick={stop}>
        Stop
      </Button>
    </>
  );

  const dock = (
    <Composer
      size="compact"
      label="Instruction for the agent"
      placeholder="Tell the agent what to do next..."
      value={draft}
      onValueChange={setDraft}
      onSubmit={submitInstruction}
      disabled={run.status === "stopped"}
    />
  );

  return (
    <DockLayout width="default" dock={dock}>
      <PageFrame>
        <PageHeader
          eyebrow="AiDEX Native agent"
          title={taskName}
          description={
            taskName === DEFAULT_TASK ? `Fix the refresh flow, update tests, and review the patch. ${SAMPLE_RUN_NOTE}` : SAMPLE_RUN_NOTE
          }
          actions={actions}
          meta={meta}
        />
        <Split divided>
          <PlanSection completed={run.completed} stepCount={run.stepCount} />
          <ActivitySection
            log={run.log}
            canAdvance={run.status === "running"}
            onAdvance={advance}
            logRef={logRef}
            reviewRef={reviewRef}
          />
        </Split>
        <PermissionsSection />
      </PageFrame>
    </DockLayout>
  );
}
