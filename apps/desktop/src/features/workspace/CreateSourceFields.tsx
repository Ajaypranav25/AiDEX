import { CloudIcon, FolderIcon, GitBranchIcon, UsbDriveIcon } from "@aidex/ui/icons";
import { Button, ChoiceCardGroup, Select, Stack, TextField, type ChoiceCardOption } from "@aidex/ui/primitives";
import { useToast } from "../../app/providers/toast";
import type { CreateWorkspaceForm, FormErrors, WorkspaceSource } from "./createWorkspaceForm";
import { driveProjectOptions } from "./sample";

const sourceOptions = [
  {
    value: "local",
    title: "Local repository",
    description: "Open a folder that is already on this computer.",
    icon: <FolderIcon />,
  },
  {
    value: "clone",
    title: "Git clone",
    description: "Clone a repository from a URL into the drive.",
    icon: <CloudIcon />,
  },
  {
    value: "drive",
    title: "Drive workspace",
    description: "Use a project stored on the AiDEX drive.",
    icon: <UsbDriveIcon />,
  },
] as const satisfies readonly ChoiceCardOption<WorkspaceSource>[];

type Props = {
  form: CreateWorkspaceForm;
  errors: FormErrors;
  onChange: (patch: Partial<CreateWorkspaceForm>) => void;
};

/** Source picker plus the fields that source needs: a folder, a repository, or a drive project. */
export function CreateSourceFields({ form, errors, onChange }: Props) {
  const notify = useToast();

  return (
    <Stack gap={6}>
      <ChoiceCardGroup
        label="Where is the project?"
        columns={3}
        options={sourceOptions}
        value={form.source}
        onValueChange={(source) => onChange({ source })}
      />
      {form.source === "local" ? (
        <TextField
          label="Folder path"
          size="md"
          placeholder="~/Projects/my-project"
          spellCheck={false}
          autoComplete="off"
          value={form.folderPath}
          error={errors.folderPath}
          onChange={(event) => onChange({ folderPath: event.target.value })}
          trailing={
            <Button
              variant="ghost"
              size="md"
              icon={<FolderIcon />}
              onClick={() => notify("Preview: the folder picker opens when AiDEX Core is connected. Type a path for now.")}
            >
              Browse
            </Button>
          }
        />
      ) : null}
      {form.source === "clone" ? (
        <>
          <TextField
            label="Repository URL"
            size="md"
            type="url"
            placeholder="https://github.com/you/repository.git"
            spellCheck={false}
            autoComplete="off"
            value={form.repositoryUrl}
            error={errors.repositoryUrl}
            onChange={(event) => onChange({ repositoryUrl: event.target.value })}
          />
          <TextField
            label="Branch"
            size="md"
            leadingIcon={<GitBranchIcon />}
            spellCheck={false}
            autoComplete="off"
            hint="The branch AiDEX checks out after cloning."
            value={form.branch}
            error={errors.branch}
            onChange={(event) => onChange({ branch: event.target.value })}
          />
        </>
      ) : null}
      {form.source === "drive" ? (
        <Select
          label="Project on the drive"
          hint="Projects in AiDEX:/projects that are not workspaces yet."
          options={driveProjectOptions}
          value={form.driveProject}
          onChange={(event) => onChange({ driveProject: event.target.value })}
        />
      ) : null}
    </Stack>
  );
}
