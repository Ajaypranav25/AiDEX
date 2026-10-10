"use client";

import { useEffect, useRef, useState, type FormEvent } from "react";
import { ToastRegion } from "@aidex/ui/components";
import { ArrowLeftIcon, ArrowRightIcon, ExternalLinkIcon } from "@aidex/ui/icons";
import { SetupFooter, SetupWindow, StepTrack } from "@aidex/ui/installer";
import { Badge, Button, Stack } from "@aidex/ui/primitives";
import { isEligibleVolume } from "@aidex/client/dto/drive";
import { fitsOnDrive, initialSetup, type SetupState } from "./lib/setup";
import { LAST_STEP, steps } from "./lib/steps";
import { useDrives } from "./lib/useDrives";
import { useInstallRun } from "./lib/useInstallRun";
import { useToasts } from "./lib/useToasts";
import { AgentsStep } from "./steps/agent-selection/AgentsStep";
import { ConfigurationStep } from "./steps/configuration/ConfigurationStep";
import { DriveStep } from "./steps/drive-selection/DriveStep";
import { passwordProblem } from "./steps/encryption/sample";
import { SecurityStep } from "./steps/encryption/SecurityStep";
import { InstallStep } from "./steps/installation/InstallStep";
import { ModelsStep } from "./steps/model-selection/ModelsStep";

const FORM_ID = "setup-step";
const NOTE_ID = "setup-footer-note";
const SECURITY_STEP = steps.findIndex((step) => step.id === "security");

/**
 * AiDEX Setup. State lives here. Drives come from `list_drives`; the Install step initializes the
 * chosen removable drive with `init_drive` (AiDEX folders and manifest only; never formats or deletes).
 */
export default function App() {
  const [state, setState] = useState<SetupState>(initialSetup);
  const [stepIndex, setStepIndex] = useState(0);
  const [furthest, setFurthest] = useState(0);
  const [customPathError, setCustomPathError] = useState<string>();
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [passwordError, setPasswordError] = useState<string>();
  const [passwordAccepted, setPasswordAccepted] = useState(false);
  const { toasts, notify, dismiss } = useToasts();
  const run = useInstallRun();
  const drives = useDrives();
  const volume = drives.volumes.find((candidate) => candidate.root === state.driveRoot) ?? null;
  const titleRef = useRef<HTMLHeadingElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const customPathRef = useRef<HTMLInputElement>(null);
  const passwordRef = useRef<HTMLInputElement>(null);
  // A new view starts at the top with focus on its heading; compared against the previous view so StrictMode's double effect is harmless.
  const viewKey = `${stepIndex}:${run.phase}`;
  const previousView = useRef(viewKey);

  useEffect(() => {
    if (previousView.current === viewKey) return;
    previousView.current = viewKey;
    contentRef.current?.scrollTo({ top: 0 });
    titleRef.current?.focus({ preventScroll: true });
  }, [viewKey]);

  const update = (patch: Partial<SetupState>) => {
    setState((current) => ({ ...current, ...patch }));
    if ("vault" in patch) {
      setPasswordAccepted(false);
      setPasswordError(undefined);
    }
  };

  const goTo = (index: number) => {
    // Passwords never outlive the Security step.
    if (stepIndex === SECURITY_STEP && index !== SECURITY_STEP) {
      setPassword("");
      setConfirmation("");
      setPasswordError(undefined);
    }
    setStepIndex(index);
    setFurthest((current) => Math.max(current, index));
  };

  const stepIsValid = (): boolean => {
    if (steps[stepIndex].id === "drive" && !(volume && isEligibleVolume(volume))) {
      notify("Choose a removable drive or portable SSD to continue.", "danger");
      return false;
    }
    if (steps[stepIndex].id === "models" && state.models.includes("custom") && !state.customModelPath.trim()) {
      setCustomPathError("Enter the GGUF file path, or clear Bring your own GGUF.");
      customPathRef.current?.focus();
      return false;
    }
    if (steps[stepIndex].id === "security" && state.vault === "password" && !(passwordAccepted && !password && !confirmation)) {
      const problem = passwordProblem(password, confirmation);
      setPasswordError(problem);
      if (problem) {
        passwordRef.current?.focus();
        return false;
      }
      setPasswordAccepted(true);
    }
    return true;
  };

  const handleContinue = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (stepIndex < LAST_STEP && stepIsValid()) goTo(stepIndex + 1);
  };

  const restart = () => {
    run.reset();
    setFurthest(0);
    goTo(0);
  };

  const stepProps = { state, update, notify, titleRef, volume };
  const vaultReady = state.vault === "none" || passwordAccepted;
  const canInstall = volume !== null && isEligibleVolume(volume) && fitsOnDrive(state, volume) && vaultReady;

  const track = steps.map((step, index) => ({
    id: step.id,
    title: step.title,
    status: index < stepIndex ? ("complete" as const) : index === stepIndex ? ("current" as const) : ("upcoming" as const),
    reachable: index <= furthest && run.phase !== "running",
  }));

  return (
    <>
      <SetupWindow
        title="AiDEX Setup"
        subtitle="Portable environment installer"
        badge={<Badge tone="neutral">Initializes the drive · components arrive later</Badge>}
        contentRef={contentRef}
        steps={
          <StepTrack label="Installation steps" steps={track} onSelect={(id) => goTo(steps.findIndex((step) => step.id === id))} />
        }
        footer={renderFooter()}
      >
        <form id={FORM_ID} onSubmit={handleContinue} noValidate>
          {/* The form scopes Enter-to-continue; render functions (not inner components) keep inputs mounted across renders. */}
          <Stack gap={7}>{renderStepBody()}</Stack>
        </form>
      </SetupWindow>
      <ToastRegion toasts={toasts} onDismiss={dismiss} />
    </>
  );

  function renderStepBody() {
    switch (steps[stepIndex].id) {
      case "drive":
        return <DriveStep {...stepProps} drives={drives} />;
      case "setup":
        return <ConfigurationStep {...stepProps} />;
      case "models":
        return (
          <ModelsStep
            {...stepProps}
            customPathRef={customPathRef}
            customPathError={customPathError}
            update={(patch) => {
              setCustomPathError(undefined);
              update(patch);
            }}
          />
        );
      case "agents":
        return <AgentsStep {...stepProps} />;
      case "security":
        return (
          <SecurityStep
            {...stepProps}
            password={password}
            confirmation={confirmation}
            onPasswordChange={(value) => {
              setPassword(value);
              setPasswordError(undefined);
            }}
            onConfirmationChange={(value) => {
              setConfirmation(value);
              setPasswordError(undefined);
            }}
            passwordAccepted={passwordAccepted}
            error={passwordError}
            passwordRef={passwordRef}
          />
        );
      case "install":
        return (
          <InstallStep
            state={state}
            titleRef={titleRef}
            volume={volume}
            phase={run.phase}
            tasks={run.tasks}
            completed={run.completed}
            report={run.report}
            error={run.error}
            passwordAccepted={passwordAccepted}
          />
        );
    }
  }

  function renderFooter() {
    const back =
      stepIndex > 0 ? (
        <Button variant="outline" icon={<ArrowLeftIcon />} onClick={() => goTo(stepIndex - 1)}>
          Back
        </Button>
      ) : null;

    if (stepIndex < LAST_STEP) {
      return (
        <SetupFooter noteId={NOTE_ID} note={`Step ${stepIndex + 1} of ${steps.length} · ${steps[stepIndex].title}`}>
          {back}
          <Button type="submit" form={FORM_ID} variant="primary" trailing={<ArrowRightIcon />}>
            Continue
          </Button>
        </SetupFooter>
      );
    }
    if (run.phase === "running") {
      // Initialization is short and safe to retry, but stopping midway is not offered.
      return (
        <SetupFooter noteId={NOTE_ID} note={`Setting up ${volume?.root ?? "the drive"}…`}>
          {null}
        </SetupFooter>
      );
    }
    if (run.phase === "failed") {
      return (
        <SetupFooter noteId={NOTE_ID} note="Setup stopped. Fix the problem, then try again.">
          <Button variant="outline" icon={<ArrowLeftIcon />} onClick={restart}>
            Start over
          </Button>
          <Button variant="primary" onClick={() => void run.start(state)}>
            Try again
          </Button>
        </SetupFooter>
      );
    }
    if (run.phase === "finished") {
      return (
        <SetupFooter noteId={NOTE_ID} note={`AiDEX drive ready at ${run.report?.root ?? volume?.root ?? "the drive"}.`}>
          <Button variant="outline" onClick={restart}>
            New installation
          </Button>
          <Button
            variant="primary"
            trailing={<ExternalLinkIcon />}
            onClick={() =>
              notify("The AiDEX app is not on the drive yet; it is copied there once the portable bundle (issue #26) is available.")
            }
          >
            Launch AiDEX
          </Button>
        </SetupFooter>
      );
    }
    const blocked = !volume
      ? "Choose a drive on the Drive step first."
      : !fitsOnDrive(state, volume)
      ? "Not enough free space on the selected drive."
      : !vaultReady
        ? "Set the vault password on the Security step first."
        : `Step ${steps.length} of ${steps.length} · Review`;
    return (
      <SetupFooter noteId={NOTE_ID} note={blocked}>
        {back}
        <Button
          variant="primary"
          trailing={<ArrowRightIcon />}
          disabled={!canInstall}
          aria-describedby={canInstall ? undefined : NOTE_ID}
          onClick={() => void run.start(state)}
        >
          Set up drive
        </Button>
      </SetupFooter>
    );
  }
}
