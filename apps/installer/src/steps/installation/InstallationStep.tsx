import { useRef } from "react";
import { ActivityLog, KeyValueList, MediaItem, SectionHeader, StepList, type ActivityEntry, type Step } from "@aidex/ui/components";
import { InfoIcon, PlayIcon } from "@aidex/ui/icons";
import { Button, Card, ProgressBar, Split, Stack } from "@aidex/ui/primitives";
import type { RunPhase } from "../../lib/usePreviewRun";
import type { Notify } from "../../lib/useToasts";
import { driveById, estimateGb, formatGb, RUNTIME, selectedItems, type SetupSelections } from "../../lib/setup";
import { CompletionPanel } from "../completion/CompletionPanel";
import { previewTasks } from "./sample";

type Props = {
  selections: SetupSelections;
  run: { phase: RunPhase; completed: number; entries: readonly ActivityEntry[]; start: () => void; reset: () => void };
  notify: Notify;
};

const PROFILE_LABEL = { lite: "Lite", developer: "Developer", custom: "Custom" } as const;

function names(selections: SetupSelections, group: Parameters<typeof selectedItems>[1]): string {
  const items = selectedItems(selections, group).map((item) => item.name);
  return items.length ? items.join(", ") : "None";
}

function summary(selections: SetupSelections) {
  const drive = driveById(selections.driveId);
  const extensions = [...selectedItems(selections, "skills"), ...selectedItems(selections, "mcp")].map((item) => item.name);
  return [
    { label: "Drive", value: drive ? `${drive.name} · ${formatGb(drive.freeGb)} free` : "Not chosen" },
    { label: "Profile", value: PROFILE_LABEL[selections.profile] },
    { label: "Models", value: names(selections, "models") },
    { label: "Runtime", value: `${RUNTIME.name} with ${names(selections, "backends")}` },
    { label: "Agents", value: names(selections, "agents") },
    { label: "Extensions", value: extensions.length ? extensions.join(", ") : "None" },
    { label: "Toolchains", value: names(selections, "toolchains") },
    { label: "Vault owner", value: selections.ownerName.trim() || "Not set" },
    { label: "Estimated size", value: formatGb(estimateGb(selections.components)) },
  ];
}

function stepStatus(index: number, phase: RunPhase, completed: number): Step["status"] {
  if (index < completed) return "done";
  return phase === "running" && index === completed ? "active" : "pending";
}

/** Screen 34: the plan summary, then a timer-driven visual preview of installation that writes nothing. */
export function InstallationStep({ selections, run, notify }: Props) {
  const driveName = driveById(selections.driveId)?.name ?? "the drive";
  const steps: Step[] = previewTasks.map((task, index) => {
    const status = stepStatus(index, run.phase, run.completed);
    return { title: task.title, description: status === "done" ? "Simulated · nothing written" : task.plan, status };
  });
  // Starting (or restarting) unmounts the button that was pressed, so focus lands on the run's step list.
  const stepsRef = useRef<HTMLOListElement>(null);
  const startRun = (restart: boolean) => {
    if (restart) run.reset();
    run.start();
    requestAnimationFrame(() => stepsRef.current?.focus());
  };

  return (
    <Stack gap={10}>
      <section>
        <Stack gap={4}>
          <SectionHeader title="Prepare drive" meta="Review your choices" />
          <KeyValueList items={summary(selections)} />
        </Stack>
      </section>
      <section>
        <Stack gap={5}>
          <SectionHeader
            title="Preview run"
            actions={
              run.phase === "idle" ? (
                <Button variant="primary" icon={<PlayIcon />} onClick={() => startRun(false)}>
                  Start preview run
                </Button>
              ) : null
            }
          />
          <Card padding="md">
            <MediaItem
              media={<InfoIcon size={22} />}
              title="This is a visual preview"
              description={`The run below only animates each stage. It does not format, copy to, encrypt, or verify ${driveName}.`}
            />
          </Card>
          {run.phase === "idle" ? null : (
            <Stack gap={5}>
              <ProgressBar label="Preview run progress" value={run.completed} max={previewTasks.length} showCount />
              <Split emphasis="equal" divided>
                <StepList ref={stepsRef} tabIndex={-1} aria-label="Preview run steps" steps={steps} />
                <ActivityLog entries={run.entries} live aria-label="Preview run log" />
              </Split>
            </Stack>
          )}
          {run.phase === "finished" ? (
            <CompletionPanel
              driveName={driveName}
              notify={notify}
              onRunAgain={() => startRun(true)}
            />
          ) : null}
        </Stack>
      </section>
    </Stack>
  );
}
