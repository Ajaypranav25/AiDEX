import { MIN_PASSWORD_LENGTH } from "../steps/encryption/sample";
import { driveById, estimateGb, formatGb, hasChatModel, type SetupSelections } from "./setup";

export type StepId = "drive" | "configuration" | "models" | "agents" | "encryption" | "installation";

export type WizardStep = {
  id: StepId;
  /** Short name for the step rail. */
  label: string;
  /** Page title. */
  title: string;
  description: string;
};

export const wizardSteps: readonly WizardStep[] = [
  { id: "drive", label: "Welcome and drive", title: "Take your AI workspace with you.", description: "" },
  {
    id: "configuration",
    label: "Configuration",
    title: "Installation configuration",
    description: "Choose how much of AiDEX to put on the drive. You can fine-tune models and agents in the next steps.",
  },
  {
    id: "models",
    label: "Models and runtime",
    title: "Local models and runtime",
    description: "Pick the models and acceleration backends to carry with you. Everything runs offline from the drive.",
  },
  {
    id: "agents",
    label: "Agents and extensions",
    title: "Agents and extensions",
    description: "AiDEX Native is always included. External agents are optional adapters that use your own accounts.",
  },
  {
    id: "encryption",
    label: "Encryption and identity",
    title: "Encryption and identity",
    description: "Protect the workspace vault on the drive with a password and a recovery key.",
  },
  {
    id: "installation",
    label: "Install and verify",
    title: "Installation and verification",
    description: "Review your choices, then watch a visual preview of how AiDEX will prepare the drive.",
  },
];

export type StepContext = {
  selections: SetupSelections;
  /** Whether the vault password fields on the encryption step currently pass validation. */
  passwordReady: boolean;
};

/** Why the user cannot continue from a step, or null when the step is complete. */
export function blockingReason(step: StepId, { selections, passwordReady }: StepContext): string | null {
  const drive = driveById(selections.driveId);
  const fits = drive ? estimateGb(selections.components) <= drive.freeGb : false;
  switch (step) {
    case "drive":
      return drive ? null : "Choose a drive to continue.";
    case "configuration":
    case "agents":
      return fits ? null : `The selection needs more than the ${formatGb(drive?.freeGb ?? 0)} free on the drive.`;
    case "models":
      if (!hasChatModel(selections)) return "Choose at least one chat model to continue.";
      return fits ? null : `The selection needs more than the ${formatGb(drive?.freeGb ?? 0)} free on the drive.`;
    case "encryption":
      if (!selections.ownerName.trim()) return "Enter the workspace owner name to continue.";
      if (!passwordReady) return `Set a vault password of at least ${MIN_PASSWORD_LENGTH} characters, rated Fair or better, and confirm it.`;
      return selections.recoveryKeySaved ? null : "Confirm that you saved the recovery key.";
    case "installation":
      return null;
  }
}

/** One-line recap of a completed step for the step rail. */
export function stepSummary(step: StepId, { selections }: StepContext): string | undefined {
  const count = (group: keyof SetupSelections["components"]) => selections.components[group].length;
  switch (step) {
    case "drive":
      return driveById(selections.driveId)?.name;
    case "configuration":
      return `${selections.profile[0].toUpperCase()}${selections.profile.slice(1)} · ${formatGb(estimateGb(selections.components))}`;
    case "models":
      return `${count("models")} models · ${count("backends")} backends`;
    case "agents":
      return `${count("agents")} agents · ${count("skills") + count("mcp")} extensions`;
    case "encryption":
      return selections.ownerName.trim() ? `Owner: ${selections.ownerName.trim()}` : undefined;
    case "installation":
      return undefined;
  }
}
