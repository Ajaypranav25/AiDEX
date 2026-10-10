"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { MediaItem, ToastRegion } from "@aidex/ui/components";
import { ArrowLeftIcon, ArrowRightIcon, EyeIcon, InfoIcon } from "@aidex/ui/icons";
import { Stepper, WizardFooter, WizardLayout, type StepperStep } from "@aidex/ui/installer";
import { Badge, Button, Card } from "@aidex/ui/primitives";
import { applyProfile, initialSelections, setPart, toggleComponent, type SetupSelections } from "./lib/setup";
import { blockingReason, stepSummary, wizardSteps } from "./lib/steps";
import { usePreviewRun, type RunPhase } from "./lib/usePreviewRun";
import { useToasts } from "./lib/useToasts";
import { previewTasks } from "./steps/installation/sample";
import { StepContent, type StepActions } from "./steps/StepContent";

const FOOTER_STATUS_ID = "wizard-footer-status";

const LAST_STEP_STATUS: Record<RunPhase, string> = {
  idle: "Start the preview run to see each installation stage.",
  running: "Preview run in progress. Back is available when it finishes.",
  finished: "Preview finished. Nothing was written to your drive.",
};

/** AiDEX Setup preview: a six-step wizard whose state lives here. It never scans, formats, encrypts, or writes to a drive. */
export default function App() {
  const [stepIndex, setStepIndex] = useState(0);
  const [selections, setSelections] = useState<SetupSelections>(initialSelections);
  const [passwordReady, setPasswordReady] = useState(false);
  const { toasts, notify, dismiss } = useToasts();
  const run = usePreviewRun(previewTasks);
  const titleRef = useRef<HTMLHeadingElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  // Compared against the current step rather than a first-mount flag, which StrictMode's double effect defeats.
  const previousStep = useRef(stepIndex);

  // On step change, start the new step at the top and move focus to its heading.
  useEffect(() => {
    if (previousStep.current === stepIndex) return;
    previousStep.current = stepIndex;
    contentRef.current?.scrollTo({ top: 0 });
    titleRef.current?.focus({ preventScroll: true });
  }, [stepIndex]);

  const actions = useMemo<StepActions>(
    () => ({
      setDrive: (driveId) => setSelections((current) => ({ ...current, driveId })),
      setProfile: (profile) => setSelections((current) => applyProfile(current, profile)),
      setPart: (partId, on) => setSelections((current) => setPart(current, partId, on)),
      toggle: (group, id, on) => setSelections((current) => toggleComponent(current, group, id, on)),
      setOwnerName: (ownerName) => setSelections((current) => ({ ...current, ownerName })),
      setRecoveryKeySaved: (recoveryKeySaved) => setSelections((current) => ({ ...current, recoveryKeySaved })),
      setPasswordReady,
    }),
    [],
  );

  const step = wizardSteps[stepIndex] ?? wizardSteps[0];
  const context = { selections, passwordReady };
  const reason = blockingReason(step.id, context);
  const running = run.phase === "running";
  const isLast = stepIndex === wizardSteps.length - 1;

  const goTo = (index: number) => {
    // Leaving the last step discards the preview run so it reflects any changed choices next time.
    if (isLast) run.reset();
    setStepIndex(index);
  };

  const stepperSteps: StepperStep[] = wizardSteps.map((item, index) => ({
    id: item.id,
    title: item.label,
    status: index < stepIndex ? "complete" : index === stepIndex ? "current" : "upcoming",
    description: index < stepIndex ? stepSummary(item.id, context) : undefined,
  }));

  const status = isLast ? LAST_STEP_STATUS[run.phase] : (reason ?? `Step ${stepIndex + 1} of ${wizardSteps.length}`);

  return (
    <>
      <WizardLayout
        title="Setup"
        badge={
          <Badge tone="neutral" icon={<EyeIcon />}>
            Preview — nothing is written to your drive
          </Badge>
        }
        steps={
          <Stepper
            label="Setup steps"
            steps={stepperSteps}
            onSelect={running ? undefined : (id) => goTo(wizardSteps.findIndex((item) => item.id === id))}
          />
        }
        aside={
          <Card padding="md">
            <MediaItem
              media={<InfoIcon size={20} />}
              title="Preview only"
              description="This installer never scans, formats, encrypts, or writes to a drive. Drives, host details, and results are sample data."
            />
          </Card>
        }
        contentRef={contentRef}
        footer={
          <WizardFooter
            statusId={FOOTER_STATUS_ID}
            status={status}
            back={
              <Button
                variant="ghost"
                icon={<ArrowLeftIcon />}
                disabled={stepIndex === 0 || running}
                aria-describedby={running ? FOOTER_STATUS_ID : undefined}
                onClick={() => goTo(stepIndex - 1)}
              >
                Back
              </Button>
            }
            next={
              isLast ? null : (
                <Button
                  variant="primary"
                  trailing={<ArrowRightIcon />}
                  disabled={reason !== null}
                  aria-describedby={reason ? FOOTER_STATUS_ID : undefined}
                  onClick={() => goTo(stepIndex + 1)}
                >
                  Next
                </Button>
              )
            }
          />
        }
      >
        <StepContent stepId={step.id} selections={selections} actions={actions} run={run} notify={notify} titleRef={titleRef} />
      </WizardLayout>
      <ToastRegion toasts={toasts} onDismiss={dismiss} />
    </>
  );
}
