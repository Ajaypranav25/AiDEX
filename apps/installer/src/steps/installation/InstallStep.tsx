import { useState } from "react";
import { ActivityLog, KeyValueList, Notice, PageHeader, SectionHeader } from "@aidex/ui/components";
import { Button, Heading, ProgressBar, Stack, Text } from "@aidex/ui/primitives";
import { driveById, estimateGb, fitsOnDrive, formatGb, installPath, type SetupState } from "../../lib/setup";
import { stepKicker } from "../../lib/steps";
import { previewTasks, type RunPhase } from "../../lib/usePreviewRun";
import type { StepProps } from "../types";

type Props = Omit<StepProps, "update" | "notify"> & {
  phase: RunPhase;
  percent: number;
  taskIndex: number;
  passwordAccepted: boolean;
};

const PRESET_LABEL = { lite: "Lite", developer: "Developer", custom: "Custom" } as const;
const MODEL_LABEL = { gemma: "Gemma 4", embedding: "Embeddings", custom: "Custom GGUF" } as const;

function summary(state: SetupState, passwordAccepted: boolean) {
  const drive = driveById(state.driveId);
  const tools = [...state.agents.map((agent) => (agent === "claude" ? "claude code" : agent)), ...state.extensions];
  const vault =
    state.vault === "none" ? "No password (unencrypted)" : passwordAccepted ? "Password-protected" : "Password not set yet";
  return [
    { label: "Drive", value: `${drive.name} (${drive.letter})` },
    { label: "Install folder", value: installPath(state) },
    { label: "Installation", value: PRESET_LABEL[state.preset] },
    { label: "Models", value: state.models.map((model) => MODEL_LABEL[model]).join(", ") || "None (cloud-ready)" },
    { label: "Agents & extras", value: tools.length ? tools.join(", ") : "AiDEX Native only" },
    { label: "Vault", value: vault },
  ];
}

/** Step 6: review the setup, then watch a visual preview of the install. Nothing is ever written to the drive. */
export function InstallStep({ state, titleRef, phase, percent, taskIndex, passwordAccepted }: Props) {
  const [showDetails, setShowDetails] = useState(false);
  const drive = driveById(state.driveId);
  const detailsToggle = (
    <Button variant="ghost" size="md" aria-expanded={showDetails} onClick={() => setShowDetails((open) => !open)}>
      {showDetails ? "Hide" : "Show"} {phase === "finished" ? "verification details" : "installation details"}
    </Button>
  );

  if (phase === "review") {
    return (
      <>
        <PageHeader
          eyebrow={stepKicker(5)}
          title="Ready to install"
          description="Confirm your configuration before preparing the portable drive."
          titleRef={titleRef}
        />
        <Stack gap={3}>
          <SectionHeader title="Your setup" level={3} meta={`About ${formatGb(estimateGb(state))}`} />
          <KeyValueList items={summary(state, passwordAccepted)} />
        </Stack>
        <Notice>Starting the install runs a visual preview only: no files are written, drives formatted, or packages downloaded.</Notice>
        {fitsOnDrive(state) ? null : (
          <Notice tone="danger" role="alert">
            Not enough free space on {drive.name}. Choose another drive or a smaller installation.
          </Notice>
        )}
      </>
    );
  }

  if (phase === "running") {
    const entries = previewTasks.slice(0, taskIndex + 1).map((task, index) => ({
      id: task,
      time: `${index + 1}/${previewTasks.length}`,
      text: `${index === taskIndex ? "Simulating" : "Simulated"}: ${task}`,
    }));
    return (
      <>
        <PageHeader
          eyebrow="Installing · preview"
          title="Installing AiDEX"
          description="Each stage below is simulated. Nothing is written to the drive."
          titleRef={titleRef}
        />
        <Stack gap={4}>
          <Heading level={2} size="title">
            {percent}%
          </Heading>
          <ProgressBar label="Installation preview progress" value={percent} />
          <Text>{previewTasks[taskIndex]}</Text>
          <Text tone="muted">Simulated setup · {drive.name} ({drive.letter})</Text>
        </Stack>
        <Stack gap={3} align="start">
          {detailsToggle}
          {showDetails ? <ActivityLog live entries={entries} /> : null}
        </Stack>
      </>
    );
  }

  // Simulated runs must never be reported as a successful setup (docs/requirements.md #8).
  const checks = [
    { label: "AiDEX runtime", value: "Not run in preview" },
    { label: "Local model selections", value: "Not run in preview" },
    { label: "Agent packages", value: "Not run in preview" },
    { label: "Vault configuration", value: state.vault === "none" ? "No password · would be unencrypted" : "Not created in preview" },
  ];
  return (
    <>
      <PageHeader
        eyebrow="Preview finished"
        title="Installation preview complete"
        description="The simulated run finished. Nothing was written, so AiDEX is not installed on this drive yet."
        titleRef={titleRef}
      />
      <Notice tone="success" variant="banner">
        Preview complete. {drive.name} ({drive.letter}) was not modified.
      </Notice>
      <Stack gap={3}>
        <SectionHeader title="Verification summary" level={3} />
        <KeyValueList items={checks} />
      </Stack>
      <Stack gap={3} align="start">
        {detailsToggle}
        {showDetails ? (
          <ActivityLog
            entries={[
              { id: "drive", time: "drive", text: `${drive.letter} ${drive.name}` },
              { id: "mode", time: "mode", text: PRESET_LABEL[state.preset] },
              { id: "fs", time: "files", text: "Filesystem not modified" },
              { id: "verify", time: "verify", text: "Runtime verification not run" },
              { id: "vault", time: "vault", text: "Vault encryption not performed" },
            ]}
          />
        ) : null}
      </Stack>
    </>
  );
}
