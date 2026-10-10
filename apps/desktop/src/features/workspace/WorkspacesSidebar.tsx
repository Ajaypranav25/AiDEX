import { SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { FolderIcon, FolderPlusIcon, PlusIcon } from "@aidex/ui/icons";
import { useNavigation } from "../../app/providers/navigation";
import { usePreferences } from "../../app/providers/preferences";
import { SectionSidebar } from "../../layouts/SectionSidebar";
import { sampleWorkspaces, type SampleWorkspace } from "../../lib/sample/workspaces";
import { filterOf } from "./workspaceFilters";

export function WorkspacesSidebar() {
  const { route, navigate, replace } = useNavigation();
  const { setWorkspaceId } = usePreferences();
  const showingAll = route.page === "workspaces" && filterOf(route.tab) === "all";

  const openWorkspace = (id: string) => {
    setWorkspaceId(id);
    navigate({ page: "workspace", id });
  };

  const workspaceItem = ({ id, name, Icon, iconTone }: SampleWorkspace) => (
    <SidebarItem key={id} label={name} icon={<Icon />} iconTone={iconTone} onSelect={() => openWorkspace(id)} />
  );

  return (
    <SectionSidebar
      title="Workspaces"
      action={{ label: "Add workspace", icon: <PlusIcon />, onClick: () => navigate({ page: "workspace-create" }) }}
    >
      <SidebarSection title="Browse">
        <SidebarItem
          label="All workspaces"
          icon={<FolderIcon />}
          active={showingAll}
          count={sampleWorkspaces.length}
          onSelect={() => replace({ page: "workspaces" })}
        />
        <SidebarItem
          label="Create or import"
          icon={<FolderPlusIcon />}
          onSelect={() => navigate({ page: "workspace-create" })}
        />
      </SidebarSection>
      <SidebarSection title="Pinned">{sampleWorkspaces.filter((w) => w.pinned).map(workspaceItem)}</SidebarSection>
      <SidebarSection title="Recent">{sampleWorkspaces.filter((w) => !w.pinned).map(workspaceItem)}</SidebarSection>
    </SectionSidebar>
  );
}
