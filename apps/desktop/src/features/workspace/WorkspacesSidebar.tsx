import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { FolderIcon, PlusIcon } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { useRpc } from "../../hooks/useRpc";
import { SectionSidebar } from "../../layouts/SectionSidebar";
export function WorkspacesSidebar() {
  const { route, navigate } = useNavigation();
  const workspaces = useRpc("workspace.list");
  return (
    <SectionSidebar title="Workspaces">
      <SidebarSection title="Projects">
        <SidebarItem
          label="All workspaces"
          icon={<FolderIcon />}
          active={route.page === "workspaces"}
          onSelect={() => navigate({ page: "workspaces" })}
        />
        <SidebarItem
          label="Add workspace"
          icon={<PlusIcon />}
          active={route.page === "workspace-create"}
          onSelect={() => navigate({ page: "workspace-create" })}
        />
        {(workspaces.data?.workspaces ?? []).map((workspace) => (
          <SidebarItem
            key={workspace.id}
            label={workspace.name}
            icon={<FolderIcon />}
            active={route.id === workspace.id}
            onSelect={() => navigate({ page: "workspace", id: workspace.id })}
          />
        ))}
      </SidebarSection>
    </SectionSidebar>
  );
}
