import type { Ref } from "react";
import { PageHeader } from "@aidex/ui/components";
import { Stack } from "@aidex/ui/primitives";
import type { ComponentGroupId, ProfileId, SetupSelections } from "../lib/setup";
import { wizardSteps, type StepId } from "../lib/steps";
import type { usePreviewRun } from "../lib/usePreviewRun";
import type { Notify } from "../lib/useToasts";
import { AgentsStep } from "./agent-selection/AgentsStep";
import { ConfigurationStep } from "./configuration/ConfigurationStep";
import { DriveStep } from "./drive-selection/DriveStep";
import { EncryptionStep } from "./encryption/EncryptionStep";
import { InstallationStep } from "./installation/InstallationStep";
import { ModelsStep } from "./model-selection/ModelsStep";
import { WelcomeHero } from "./welcome/WelcomeHero";

export type StepActions = {
  setDrive: (id: string) => void;
  setProfile: (profile: ProfileId) => void;
  setPart: (partId: string, on: boolean) => void;
  toggle: (group: ComponentGroupId, id: string, on: boolean) => void;
  setOwnerName: (name: string) => void;
  setRecoveryKeySaved: (saved: boolean) => void;
  setPasswordReady: (ready: boolean) => void;
};

type Props = {
  stepId: StepId;
  selections: SetupSelections;
  actions: StepActions;
  run: ReturnType<typeof usePreviewRun>;
  notify: Notify;
  titleRef: Ref<HTMLHeadingElement>;
};

function StepBody({ stepId, selections, actions, run, notify }: Omit<Props, "titleRef">) {
  switch (stepId) {
    case "drive":
      return <DriveStep driveId={selections.driveId} onDriveChange={actions.setDrive} notify={notify} />;
    case "configuration":
      return <ConfigurationStep selections={selections} onProfileChange={actions.setProfile} onPartChange={actions.setPart} />;
    case "models":
      return <ModelsStep selections={selections} onToggle={actions.toggle} />;
    case "agents":
      return <AgentsStep selections={selections} onToggle={actions.toggle} />;
    case "encryption":
      return (
        <EncryptionStep
          ownerName={selections.ownerName}
          onOwnerNameChange={actions.setOwnerName}
          recoveryKeySaved={selections.recoveryKeySaved}
          onRecoveryKeySavedChange={actions.setRecoveryKeySaved}
          onPasswordReadyChange={actions.setPasswordReady}
          notify={notify}
        />
      );
    case "installation":
      return <InstallationStep selections={selections} run={run} notify={notify} />;
  }
}

/** The current step's heading and body; the heading takes focus when the step changes. */
export function StepContent({ titleRef, ...props }: Props) {
  const index = wizardSteps.findIndex((step) => step.id === props.stepId);
  const step = wizardSteps[index];
  return (
    <Stack gap={8}>
      {props.stepId === "drive" ? (
        <WelcomeHero titleRef={titleRef} />
      ) : (
        <PageHeader
          eyebrow={`Step ${index + 1} of ${wizardSteps.length}`}
          title={step?.title}
          description={step?.description}
          titleRef={titleRef}
        />
      )}
      <StepBody {...props} />
    </Stack>
  );
}
