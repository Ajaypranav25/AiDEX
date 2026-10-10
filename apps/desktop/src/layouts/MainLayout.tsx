import { useEffect, useMemo, useRef, useState, type ComponentType, type ReactNode } from "react";
import { AppShell, NavRail, TopBar } from "@aidex/ui/desktop";
import { PanelLeftIcon } from "@aidex/ui/icons";
import { Avatar, IconButton, SearchField, Select, StatusIndicator, Text } from "@aidex/ui/primitives";
import { useNavigation } from "../app/providers/navigation";
import { usePreferences } from "../app/providers/preferences";
import { railIdFor, railItems, type RailId } from "../lib/navigation";
import { placeholderAccount } from "../lib/placeholders";
import { pageTitles } from "../lib/routes";
import { modelOptions } from "../lib/sample/models";
import { ShellContext, type Shell } from "./shell";

type Props = {
  Sidebar: ComponentType;
  onNewSession: () => void;
  children: ReactNode;
};

const rail = railItems.map(({ id, label, Icon }) => ({ id, label, icon: <Icon /> }));

export function MainLayout({ Sidebar, onNewSession, children }: Props) {
  const { route, navigate } = useNavigation();
  const { modelId, setModelId } = usePreferences();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const searchRef = useRef<HTMLInputElement>(null);
  const hideSidebarRef = useRef<HTMLButtonElement>(null);
  const showSidebarRef = useRef<HTMLButtonElement>(null);
  const contentRef = useRef<HTMLElement>(null);
  const sidebarToggled = useRef(false);
  const firstRoute = useRef(true);
  const pageKey = `${route.page}:${route.id ?? ""}`;

  // The toggle that was pressed unmounts, so hand focus to its counterpart.
  useEffect(() => {
    if (!sidebarToggled.current) return;
    sidebarToggled.current = false;
    (sidebarOpen ? hideSidebarRef : showSidebarRef).current?.focus();
  }, [sidebarOpen]);

  // A new page starts at the top, and its title takes focus so the change is announced.
  useEffect(() => {
    if (firstRoute.current) {
      firstRoute.current = false;
      return;
    }
    contentRef.current?.scrollTo({ top: 0 });
    contentRef.current?.querySelector<HTMLElement>("h1[tabindex='-1']")?.focus();
  }, [pageKey]);

  const toggleSidebar = (open: boolean) => {
    sidebarToggled.current = true;
    setSidebarOpen(open);
  };

  const shell = useMemo<Shell>(
    () => ({
      focusSearch: () => searchRef.current?.focus(),
      collapseSidebar: () => toggleSidebar(false),
      startNewSession: onNewSession,
      collapseButtonRef: hideSidebarRef,
    }),
    [onNewSession],
  );

  const selectRail = (id: RailId) => {
    const item = railItems.find((candidate) => candidate.id === id);
    if (item) navigate(item.route);
  };

  const header = (
    <TopBar
      start={
        <>
          {sidebarOpen ? null : (
            <IconButton ref={showSidebarRef} label="Show sidebar" icon={<PanelLeftIcon />} onClick={() => toggleSidebar(true)} />
          )}
          {route.page === "home" ? null : (
            <Text as="span" variant="label" tone="secondary">
              {pageTitles[route.page]}
            </Text>
          )}
        </>
      }
      center={<SearchField ref={searchRef} label="Search" placeholder="Search sessions, workspaces, or files..." />}
      end={
        <>
          <StatusIndicator tone="success" label="Drive connected" />
          <Select
            label="Model"
            labelVisibility="hidden"
            variant="ghost"
            options={modelOptions}
            value={modelId}
            onChange={(event) => setModelId(event.target.value)}
          />
          <Avatar name={placeholderAccount.name} size="lg" />
        </>
      }
    />
  );

  return (
    <ShellContext.Provider value={shell}>
      <AppShell
        rail={<NavRail label="Main navigation" items={rail} activeId={railIdFor(route)} onSelect={selectRail} />}
        sidebar={sidebarOpen ? <Sidebar /> : undefined}
        header={header}
        contentRef={contentRef}
      >
        {children}
      </AppShell>
    </ShellContext.Provider>
  );
}
