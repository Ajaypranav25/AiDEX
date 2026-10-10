import { useEffect, useRef, useState, type ReactNode } from "react";
import { AppShell, NavRail, TopBar } from "@aidex/ui/desktop";
import { PanelLeftIcon } from "@aidex/ui/icons";
import { Avatar, IconButton, SearchField } from "@aidex/ui/primitives";
import { navigationItems, type PageId } from "../lib/navigation";
import { pinnedEntries, placeholderAccount } from "../lib/placeholders";
import { WorkspaceSidebar } from "./WorkspaceSidebar";

type Props = {
  activePage: PageId;
  onNavigate: (page: PageId) => void;
  onNewSession: () => void;
  children: ReactNode;
};

const railItems = navigationItems.map(({ id, label, Icon }) => ({ id, label, icon: <Icon /> }));

export function MainLayout({ activePage, onNavigate, onNewSession, children }: Props) {
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [activeEntryId, setActiveEntryId] = useState(pinnedEntries[0].id);
  const searchRef = useRef<HTMLInputElement>(null);
  const hideSidebarRef = useRef<HTMLButtonElement>(null);
  const showSidebarRef = useRef<HTMLButtonElement>(null);
  const sidebarToggled = useRef(false);

  // The toggle that was pressed unmounts, so hand focus to its counterpart.
  useEffect(() => {
    if (!sidebarToggled.current) return;
    sidebarToggled.current = false;
    (sidebarOpen ? hideSidebarRef : showSidebarRef).current?.focus();
  }, [sidebarOpen]);

  const toggleSidebar = (open: boolean) => {
    sidebarToggled.current = true;
    setSidebarOpen(open);
  };

  const sidebar = sidebarOpen ? (
    <WorkspaceSidebar
      activeEntryId={activeEntryId}
      onSelectEntry={setActiveEntryId}
      onNewSession={onNewSession}
      onSearch={() => searchRef.current?.focus()}
      onCollapse={() => toggleSidebar(false)}
      collapseButtonRef={hideSidebarRef}
    />
  ) : undefined;

  const header = (
    <TopBar
      start={
        sidebarOpen ? null : (
          <IconButton ref={showSidebarRef} label="Show sidebar" icon={<PanelLeftIcon />} onClick={() => toggleSidebar(true)} />
        )
      }
      center={<SearchField ref={searchRef} label="Search" placeholder="Search sessions, workspaces, or files..." />}
      end={<Avatar name={placeholderAccount.name} size="lg" />}
    />
  );

  return (
    <AppShell
      rail={<NavRail label="Main navigation" items={railItems} activeId={activePage} onSelect={onNavigate} />}
      sidebar={sidebar}
      header={header}
    >
      {children}
    </AppShell>
  );
}
