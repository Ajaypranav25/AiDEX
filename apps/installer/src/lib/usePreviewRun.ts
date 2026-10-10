import { useCallback, useEffect, useState } from "react";

export type RunPhase = "review" | "running" | "finished";

/** What the real installer will do, in order. The preview only animates through them. */
export const previewTasks = [
  "Preparing destination directory",
  "Staging AiDEX runtime",
  "Configuring models and toolchains",
  "Applying vault settings",
  "Verifying the setup",
] as const;

const TICK_MS = 80;
const STEP_PERCENT = 2;

/**
 * Drives the visual installation preview on a timer. It never touches the drive: progress is a
 * number that climbs to 100, and the current task is derived from it.
 */
export function usePreviewRun() {
  const [phase, setPhase] = useState<RunPhase>("review");
  const [percent, setPercent] = useState(0);

  useEffect(() => {
    if (phase !== "running") return;
    if (percent >= 100) {
      setPhase("finished");
      return;
    }
    const timer = setTimeout(() => setPercent((value) => Math.min(100, value + STEP_PERCENT)), TICK_MS);
    return () => clearTimeout(timer);
  }, [phase, percent]);

  const start = useCallback(() => {
    setPercent(0);
    setPhase("running");
  }, []);

  const reset = useCallback(() => {
    setPercent(0);
    setPhase("review");
  }, []);

  const taskIndex = Math.min(previewTasks.length - 1, Math.floor(percent / (100 / previewTasks.length)));
  return { phase, percent, taskIndex, start, reset };
}
