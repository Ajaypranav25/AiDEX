import type { Ref } from "react";
import { Brand } from "@aidex/ui/components";
import { Sidebar, SidebarItem, SidebarSection } from "@aidex/ui/desktop";
import { MoreHorizontalIcon, PanelLeftIcon, PlusCircleIcon, SearchIcon } from "@aidex/ui/icons";
import { Button, IconButton, Kbd, Stack } from "@aidex/ui/primitives";
import { useModifierKeyLabel } from "../hooks/useModifierKeyLabel";
import { pinnedEntries, recentEntries, workspaceEntries } from "../lib/placeholders";

type Props = {
  activeEntryId: string;
  onSelectEntry: (id: string) => void;
  onNewSession: () => void;
  onSearch: () => void;
  onCollapse: () => void;
  collapseButtonRef: Ref<HTMLButtonElement>;
};

export function WorkspaceSidebar({
  activeEntryId,
  onSelectEntry,
  onNewSession,
  onSearch,
  onCollapse,
  collapseButtonRef,
}: Props) {
  const modifierKey = useModifierKeyLabel();

  const header = (
    <>
      <Stack direction="row" align="center" justify="between">
        <Brand />
        <Stack direction="row" gap={1}>
          <IconButton label="Jump to search" icon={<SearchIcon />} onClick={onSearch} />
          <IconButton ref={collapseButtonRef} label="Hide sidebar" icon={<PanelLeftIcon />} onClick={onCollapse} />
        </Stack>
      </Stack>
      <Button
        variant="raised"
        size="lg"
        fullWidth
        icon={<PlusCircleIcon />}
        trailing={<Kbd aria-hidden="true">{modifierKey} K</Kbd>}
        aria-keyshortcuts={modifierKey === "⌘" ? "Meta+K" : "Control+K"}
        onClick={onNewSession}
      >
        New session
      </Button>
    </>
  );

  return (
    <Sidebar label="Workspace" header={header}>
      <SidebarSection title="Pinned">
        {pinnedEntries.map(({ id, label, Icon, tone }) => (
          <SidebarItem
            key={id}
            label={label}
            icon={Icon ? <Icon /> : undefined}
            iconTone={tone}
            active={id === activeEntryId}
            onSelect={() => onSelectEntry(id)}
            actions={
              // Disabled until an item menu exists.
              <IconButton size="sm" label={`More actions for ${label}`} icon={<MoreHorizontalIcon />} disabled />
            }
          />
        ))}
      </SidebarSection>
      <SidebarSection title="Workspaces">
        {workspaceEntries.map(({ id, label, Icon, tone }) => (
          <SidebarItem
            key={id}
            label={label}
            icon={Icon ? <Icon /> : undefined}
            iconTone={tone}
            active={id === activeEntryId}
            onSelect={() => onSelectEntry(id)}
          />
        ))}
      </SidebarSection>
      <SidebarSection title="Recents">
        {recentEntries.map(({ id, label }) => (
          <SidebarItem
            key={id}
            label={label}
            size="sm"
            active={id === activeEntryId}
            onSelect={() => onSelectEntry(id)}
          />
        ))}
      </SidebarSection>
    </Sidebar>
  );
}
