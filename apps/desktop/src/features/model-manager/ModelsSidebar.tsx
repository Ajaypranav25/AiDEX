import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { useNavigation } from "../../app/providers/navigation";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { useRpc } from "../../hooks/useRpc";
export function ModelsSidebar() {
  const { route, navigate } = useNavigation();
  const models = useRpc("model.list");
  return (
    <SectionSidebar title="Models">
      <SidebarSection title="Available models">
        {models.data?.models.map((model) => {
          const id = `${model.model.provider_id}/${model.model.model_id}`;
          return (
            <SidebarItem
              key={id}
              label={model.display_name}
              active={route.page === "model" && route.id === id}
              onSelect={() => navigate({ page: "model", id })}
            />
          );
        })}
      </SidebarSection>
      <SidebarSection title="Manage">
        <SidebarItem
          label="Local inference"
          onSelect={() => navigate({ page: "models", tab: "local" })}
        />
        <SidebarItem
          label="Provider accounts"
          onSelect={() => navigate({ page: "models", tab: "providers" })}
        />
      </SidebarSection>
    </SectionSidebar>
  );
}
