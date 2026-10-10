// Options and validation for the agent settings form.
import { modelOptions } from "../../lib/sample/models";
import type { AgentConfig, AgentPermissions, Autonomy } from "./sample";

export const autonomyOptions: readonly { value: Autonomy; label: string }[] = [
  { value: "ask-every-step", label: "Ask every step" },
  { value: "ask-before-edits", label: "Ask before edits" },
  { value: "autonomous", label: "Autonomous" },
];

export const sandboxOptions = [
  { value: "strict", label: "Strict · workspace only" },
  { value: "standard", label: "Standard · workspace and temp files" },
  { value: "relaxed", label: "Relaxed · read-only host access" },
] as const;

export const fallbackModelOptions = [{ value: "none", label: "No fallback" }, ...modelOptions] as const;

export const permissionRows: readonly { key: keyof AgentPermissions; title: string; description: string }[] = [
  { key: "readFiles", title: "Read workspace files", description: "Open and search files in the selected workspace." },
  { key: "editFiles", title: "Edit files", description: "Requires approval before each set of changes is applied." },
  { key: "runShell", title: "Run shell commands", description: "Requires approval before each command runs." },
  { key: "network", title: "Network access", description: "Fetch web pages and call APIs from inside the sandbox." },
  { key: "vaultSecrets", title: "Use vault secrets", description: "Read API keys stored in the encrypted vault on the drive." },
];

export const MAX_STEPS_LIMIT = 200;
export const TIMEOUT_LIMIT_MINUTES = 240;

const wholeNumberError = (value: string, max: number, noun: string): string | undefined => {
  const parsed = Number(value);
  if (value.trim() === "" || !Number.isInteger(parsed) || parsed < 1 || parsed > max) {
    return `Enter a whole number of ${noun} from 1 to ${max}.`;
  }
  return undefined;
};

export type ConfigErrors = { maxSteps?: string; timeoutMinutes?: string };

export function validateConfig(config: AgentConfig): ConfigErrors {
  return {
    maxSteps: wholeNumberError(config.maxSteps, MAX_STEPS_LIMIT, "steps"),
    timeoutMinutes: wholeNumberError(config.timeoutMinutes, TIMEOUT_LIMIT_MINUTES, "minutes"),
  };
}

export const hasErrors = (errors: ConfigErrors) => Boolean(errors.maxSteps || errors.timeoutMinutes);
