import { useState } from "react";
import type { ActivityEntry } from "@aidex/ui/components";
import { advanceLog, baseLog, planSteps } from "./sample";

export type RunStatus = "running" | "paused" | "stopped" | "complete";

const STEP_COUNT = planSteps.length;
const INITIAL_COMPLETED = 1;

let entryCount = 0;
const clockTime = () => new Date().toLocaleTimeString("en-GB", { hour12: false });
const entry = (text: ActivityEntry["text"], tone?: ActivityEntry["tone"]): ActivityEntry => ({
  id: `demo-${++entryCount}`,
  time: clockTime(),
  text,
  tone,
});

/** Local, scripted agent run for the preview: nothing executes, but the controls behave like a live run. */
export function useDemoRun() {
  const [completed, setCompleted] = useState(INITIAL_COMPLETED);
  const [stage, setStage] = useState<"running" | "paused" | "stopped">("running");
  const [log, setLog] = useState<readonly ActivityEntry[]>(baseLog);

  const status: RunStatus = stage === "stopped" ? "stopped" : completed >= STEP_COUNT ? "complete" : stage;
  const append = (...entries: ActivityEntry[]) => setLog((current) => [...current, ...entries]);

  const togglePause = () => {
    const next = stage === "paused" ? "running" : "paused";
    setStage(next);
    append(entry(next === "paused" ? "Run paused by you" : "Run resumed"));
  };

  const stop = () => {
    setStage("stopped");
    append(entry("Run stopped by you", "danger"));
  };

  const advance = () => {
    if (completed >= STEP_COUNT) return;
    const next = completed + 1;
    setCompleted(next);
    append(entry(advanceLog[next - 2] ?? "Step complete", next === STEP_COUNT ? "success" : undefined));
  };

  const instruct = (text: string) =>
    append(entry(`You: ${text}`, "accent"), entry("Instruction noted. Preview runs don't send it to a model."));

  return { completed, stepCount: STEP_COUNT, status, log, togglePause, stop, advance, instruct };
}
