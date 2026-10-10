import { useCallback, useEffect, useState } from "react";
import type { ActivityEntry } from "@aidex/ui/components";

export type RunPhase = "idle" | "running" | "finished";

export type PreviewTask = {
  title: string;
  /** What the real installer will do in this stage. */
  plan: string;
};

const STAGE_DURATION_MS = 1100;

function clock(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  return `${String(minutes).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

/**
 * Drives the visual installation preview on a timer. It never touches the drive: each stage only
 * advances a counter and appends a log line that says the stage was simulated.
 */
export function usePreviewRun(tasks: readonly PreviewTask[]) {
  const [phase, setPhase] = useState<RunPhase>("idle");
  const [completed, setCompleted] = useState(0);
  const [entries, setEntries] = useState<readonly ActivityEntry[]>([]);

  useEffect(() => {
    if (phase !== "running") return;
    const timer = setTimeout(() => {
      const task = tasks[completed];
      const next = completed + 1;
      const elapsed = clock(Math.round((next * STAGE_DURATION_MS) / 1000));
      const stageEntry: ActivityEntry = { id: `stage-${next}`, time: elapsed, text: `${task?.title ?? "Stage"}: simulated, nothing written.` };
      const done = next >= tasks.length;
      setCompleted(next);
      setEntries((current) =>
        done
          ? [...current, stageEntry, { id: "finished", time: elapsed, text: "Preview run finished. The drive was not changed.", tone: "accent" }]
          : [...current, stageEntry],
      );
      if (done) setPhase("finished");
    }, STAGE_DURATION_MS);
    return () => clearTimeout(timer);
  }, [phase, completed, tasks]);

  const start = useCallback(() => {
    setCompleted(0);
    setEntries([{ id: "start", time: clock(0), text: "Preview run started. This is a visual walkthrough only.", tone: "accent" }]);
    setPhase("running");
  }, []);

  const reset = useCallback(() => {
    setPhase("idle");
    setCompleted(0);
    setEntries([]);
  }, []);

  return { phase, completed, entries, start, reset };
}
