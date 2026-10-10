// Form model for adding a workspace. The current setup step is mirrored into `route.tab`,
// which is how the Create sidebar shows progress.
import { driveProjectOptions } from "./sample";

export type WorkspaceSource = "local" | "clone" | "drive";

export type CreateWorkspaceForm = {
  source: WorkspaceSource;
  folderPath: string;
  repositoryUrl: string;
  branch: string;
  driveProject: string;
  /** Empty means "use the name suggested from the source". */
  name: string;
  indexRepository: boolean;
  openInAgent: boolean;
};

export type FormErrors = Errors;

export type SetupStep = "source" | "details" | "start";

export const setupSteps = [
  { value: "source", title: "Choose source", sectionId: "create-workspace-source" },
  { value: "details", title: "Workspace details", sectionId: "create-workspace-details" },
  { value: "start", title: "Start building", sectionId: "create-workspace-start" },
] as const satisfies readonly { value: SetupStep; title: string; sectionId: string }[];

export const initialForm: CreateWorkspaceForm = {
  source: "local",
  folderPath: "",
  repositoryUrl: "",
  branch: "main",
  driveProject: driveProjectOptions[0].value,
  name: "",
  indexRepository: true,
  openInAgent: false,
};

type Errors = Partial<Record<"folderPath" | "repositoryUrl" | "branch" | "name", string>>;

const REPOSITORY_URL = /^(https:\/\/\S+|git@\S+:\S+)$/;
const INVALID_NAME = /[\\/:*?"<>|]/;

/** Last path or URL segment without a `.git` suffix, e.g. "finapp" from ".../finapp.git". */
function lastSegment(value: string): string {
  const segments = value.trim().replace(/\.git$/, "").split(/[\\/:]/).filter(Boolean);
  return segments[segments.length - 1] ?? "";
}

export function suggestedName(form: CreateWorkspaceForm): string {
  if (form.source === "local") return lastSegment(form.folderPath);
  if (form.source === "clone") return lastSegment(form.repositoryUrl);
  return form.driveProject;
}

export const resolvedName = (form: CreateWorkspaceForm): string => (form.name.trim() || suggestedName(form)).trim();

export function validate(form: CreateWorkspaceForm): Errors {
  const errors: Errors = {};
  if (form.source === "local" && !form.folderPath.trim()) errors.folderPath = "Enter the folder to open.";
  if (form.source === "clone") {
    if (!form.repositoryUrl.trim()) errors.repositoryUrl = "Enter the repository URL.";
    else if (!REPOSITORY_URL.test(form.repositoryUrl.trim()))
      errors.repositoryUrl = "Use an HTTPS or SSH URL, e.g. https://github.com/you/project.git";
    if (!form.branch.trim()) errors.branch = "Enter a branch to check out.";
  }
  const name = resolvedName(form);
  if (!name) errors.name = "Give the workspace a name.";
  else if (INVALID_NAME.test(name)) errors.name = "Names can't contain \\ / : * ? \" < > or |.";
  return errors;
}

export function currentStep(form: CreateWorkspaceForm): SetupStep {
  const errors = validate(form);
  const sourceComplete = !errors.folderPath && !errors.repositoryUrl && !errors.branch;
  if (!sourceComplete) return "source";
  return errors.name ? "details" : "start";
}

export const stepOf = (tab: string | undefined): SetupStep =>
  setupSteps.find((step) => step.value === tab)?.value ?? "source";
