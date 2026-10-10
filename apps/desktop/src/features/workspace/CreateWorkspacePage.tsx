import { useEffect, useRef, useState, type FormEvent } from "react";
import { PageHeader, SectionHeader } from "@aidex/ui/components";
import { PageFrame } from "@aidex/ui/desktop";
import { ArrowRightIcon, InfoIcon } from "@aidex/ui/icons";
import { Button, Checkbox, Stack, Text, TextField } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { useToast } from "../../app/providers/toast";
import { CreateSourceFields } from "./CreateSourceFields";
import {
  currentStep,
  initialForm,
  resolvedName,
  setupSteps,
  suggestedName,
  validate,
  type CreateWorkspaceForm,
} from "./createWorkspaceForm";

const [sourceStep, detailsStep, startStep] = setupSteps;

export function CreateWorkspacePage() {
  const { route, navigate, replace, back, canGoBack } = useNavigation();
  const notify = useToast();
  const formRef = useRef<HTMLFormElement>(null);
  const [form, setForm] = useState<CreateWorkspaceForm>(initialForm);
  const [submitted, setSubmitted] = useState(false);

  const allErrors = validate(form);
  // Errors stay quiet until the first submit, then update as the person types.
  const errors = submitted ? allErrors : {};
  const step = currentStep(form);

  // The sidebar reads setup progress from the route.
  useEffect(() => {
    if (route.tab !== step) replace({ ...route, tab: step });
  }, [route, replace, step]);

  const update = (patch: Partial<CreateWorkspaceForm>) => setForm((current) => ({ ...current, ...patch }));

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmitted(true);
    if (Object.keys(allErrors).length > 0) {
      requestAnimationFrame(() => formRef.current?.querySelector<HTMLElement>("[aria-invalid='true']")?.focus());
      return;
    }
    const mode = form.openInAgent ? " and open it in Agent mode" : "";
    notify(`Preview: adding “${resolvedName(form)}”${mode} will be available when AiDEX Core is connected.`);
    navigate({ page: "workspaces" });
  };

  const cancel = () => (canGoBack ? back() : navigate({ page: "workspaces" }));

  return (
    <PageFrame width="narrow">
      <PageHeader
        title="Add a workspace"
        description="Choose where AiDEX should work."
        back={{ label: "Workspaces", onClick: () => navigate({ page: "workspaces" }) }}
      />
      <form ref={formRef} onSubmit={submit} noValidate aria-label="Add a workspace">
        <Stack gap={10}>
          <section id={sourceStep.sectionId} aria-labelledby={`${sourceStep.sectionId}-title`}>
            <Stack gap={4}>
              <SectionHeader title={<span id={`${sourceStep.sectionId}-title`}>Source</span>} meta="Step 1 of 3" />
              <CreateSourceFields form={form} errors={errors} onChange={update} />
            </Stack>
          </section>
          <section id={detailsStep.sectionId} aria-labelledby={`${detailsStep.sectionId}-title`}>
            <Stack gap={4}>
              <SectionHeader title={<span id={`${detailsStep.sectionId}-title`}>Workspace details</span>} meta="Step 2 of 3" />
              <TextField
                label="Workspace name"
                size="md"
                placeholder={suggestedName(form) || "My project"}
                hint={
                  suggestedName(form)
                    ? `Leave empty to use “${suggestedName(form)}”. Project settings are stored with your AiDEX environment.`
                    : "Project settings are stored with your AiDEX environment."
                }
                autoComplete="off"
                value={form.name}
                error={errors.name}
                onChange={(event) => update({ name: event.target.value })}
              />
            </Stack>
          </section>
          <section id={startStep.sectionId} aria-labelledby={`${startStep.sectionId}-title`}>
            <Stack gap={4}>
              <SectionHeader title={<span id={`${startStep.sectionId}-title`}>Start building</span>} meta="Step 3 of 3" />
              <Checkbox
                label="Index the repository for context and search"
                description="Builds a local index so chats and agents can find relevant code."
                checked={form.indexRepository}
                onChange={(event) => update({ indexRepository: event.target.checked })}
              />
              <Checkbox
                label="Open in Agent mode after adding"
                description="Starts an agent session in this workspace instead of a chat."
                checked={form.openInAgent}
                onChange={(event) => update({ openInAgent: event.target.checked })}
              />
              <Stack direction="row" gap={3} align="center" wrap>
                <Button type="submit" variant="primary" size="md" trailing={<ArrowRightIcon />}>
                  Add workspace
                </Button>
                <Button variant="ghost" size="md" onClick={cancel}>
                  Cancel
                </Button>
              </Stack>
            </Stack>
          </section>
        </Stack>
      </form>
      <Stack direction="row" gap={2} align="center">
        <InfoIcon size={16} aria-hidden="true" />
        <Text as="span" tone="muted">
          No files are moved or modified when you add a workspace.
        </Text>
      </Stack>
    </PageFrame>
  );
}
