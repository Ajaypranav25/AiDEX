import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { CheckCircleIcon, CircleIcon } from "@aidex/ui/icons";
import { Text } from "@aidex/ui/primitives";
import { useNavigation } from "../../app/providers/navigation";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { setupSteps, stepOf } from "./createWorkspaceForm";

/** Moves focus to the first control of a setup step on the Create page. */
function focusStep(sectionId: string) {
  const section = document.getElementById(sectionId);
  section?.scrollIntoView({ block: "start", behavior: "smooth" });
  section?.querySelector<HTMLElement>("input, select, textarea, button")?.focus({ preventScroll: true });
}

export function CreateWorkspaceSidebar() {
  const { route } = useNavigation();
  const activeIndex = setupSteps.findIndex((step) => step.value === stepOf(route.tab));

  return (
    <SectionSidebar title="New workspace">
      <SidebarSection title="Setup">
        {setupSteps.map((step, index) => {
          const done = index < activeIndex;
          return (
            <SidebarItem
              key={step.value}
              // The label carries completion so it never depends on the icon alone.
              label={`${index + 1}. ${step.title}${done ? " (done)" : ""}`}
              icon={done ? <CheckCircleIcon /> : <CircleIcon />}
              active={index === activeIndex}
              onSelect={() => focusStep(step.sectionId)}
            />
          );
        })}
      </SidebarSection>
      <Text tone="muted">Workspaces can live on this computer or on the AiDEX drive. Adding one never moves your files.</Text>
    </SectionSidebar>
  );
}
