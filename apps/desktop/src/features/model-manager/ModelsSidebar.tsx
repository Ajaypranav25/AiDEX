import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { BoxIcon, CloudIcon, PlusIcon, SettingsIcon } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { useToast } from "../../app/providers/toast";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { localModels } from "./sample";

export function ModelsSidebar() {
  const { route, navigate, replace } = useNavigation();
  const { modelId } = usePreferences();
  const notify = useToast();
  const onModelPage = route.page === "model";
  const onProviders = route.page === "models" && route.tab === "providers";

  // Switching models from a details page replaces it, so "back" still returns to the Model Center.
  const openModel = (id: string) => (onModelPage ? replace({ page: "model", id }) : navigate({ page: "model", id }));
  const openProviders = () =>
    route.page === "models" ? replace({ page: "models", tab: "providers" }) : navigate({ page: "models", tab: "providers" });

  return (
    <SectionSidebar title="Models">
      <SidebarSection title="On this drive">
        {localModels.map((model) => (
          <SidebarItem
            key={model.id}
            label={model.id === modelId ? `${model.name} (in use)` : model.name}
            icon={<BoxIcon />}
            iconTone={model.id === modelId ? "accent" : undefined}
            active={onModelPage && route.id === model.id}
            onSelect={() => openModel(model.id)}
          />
        ))}
      </SidebarSection>
      <SidebarSection title="Manage">
        <SidebarItem
          label="Import model"
          icon={<PlusIcon />}
          onSelect={() => notify("Preview: importing GGUF models will be available when AiDEX Core is connected.")}
        />
        <SidebarItem label="Provider accounts" icon={<CloudIcon />} active={onProviders} onSelect={openProviders} />
        <SidebarItem label="Runtime settings" icon={<SettingsIcon />} onSelect={() => navigate({ page: "settings" })} />
      </SidebarSection>
    </SectionSidebar>
  );
}
