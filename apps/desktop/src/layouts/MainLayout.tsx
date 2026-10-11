import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ComponentType,
  type ReactNode,
} from "react";
import { AppShell, NavRail, TopBar } from "@aidex/ui/desktop";
import {
  PanelLeftIcon,
  SearchIcon,
  BoxIcon,
  UsersIcon,
  LockIcon,
} from "@aidex/ui/icons";
import { Badge, Button, IconButton, Stack, Text } from "@aidex/ui/primitives";
import { Dialog, Notice } from "@aidex/ui/components";
import { useNavigation } from "../app/providers/navigation";
import { useAidexStatus } from "../hooks/useAidexStatus";
import { railIdFor, railItems, type RailId } from "../lib/navigation";
import { call } from "@aidex/client/rpc";
import { pageTitles, samplePages, screenKeyOf } from "../lib/routes";
import { ShellContext, type Shell } from "./shell";
import { QuickSearch } from "./QuickSearch";
import styles from "./MainLayout.module.css";

type Props = {
  Sidebar: ComponentType;
  onNewSession: () => void;
  children: ReactNode;
};

const rail = railItems.map(({ id, label, Icon }) => ({
  id,
  label,
  icon: <Icon />,
}));

export function MainLayout({ Sidebar, onNewSession, children }: Props) {
  const { route, navigate } = useNavigation();
  const [searchOpen, setSearchOpen] = useState(false);
  const [accountOpen, setAccountOpen] = useState(false);
  const [accountError, setAccountError] = useState<string>();
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const searchRef = useRef<HTMLInputElement>(null);
  const hideSidebarRef = useRef<HTMLButtonElement>(null);
  const showSidebarRef = useRef<HTMLButtonElement>(null);
  const contentRef = useRef<HTMLElement>(null);
  const sidebarToggled = useRef(false);
  const pageKey = screenKeyOf(route);
  // Compared against the current key rather than a first-mount flag, which StrictMode's double effect defeats.
  const previousPageKey = useRef(pageKey);

  // The toggle that was pressed unmounts, so hand focus to its counterpart.
  useEffect(() => {
    if (!sidebarToggled.current) return;
    sidebarToggled.current = false;
    (sidebarOpen ? hideSidebarRef : showSidebarRef).current?.focus();
  }, [sidebarOpen]);

  // A new page starts at the top, and its title takes focus so the change is announced.
  useEffect(() => {
    if (previousPageKey.current === pageKey) return;
    previousPageKey.current = pageKey;
    contentRef.current?.scrollTo({ top: 0 });
    contentRef.current
      ?.querySelector<HTMLElement>("h1[tabindex='-1']")
      ?.focus();
  }, [pageKey]);

  const toggleSidebar = (open: boolean) => {
    sidebarToggled.current = true;
    setSidebarOpen(open);
  };

  const shell = useMemo<Shell>(
    () => ({
      focusSearch: () => setSearchOpen(true),
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
            <IconButton
              ref={showSidebarRef}
              label="Show sidebar"
              icon={<PanelLeftIcon />}
              onClick={() => toggleSidebar(true)}
            />
          )}
          {route.page === "home" ? null : (
            <Text as="span" variant="label" tone="secondary">
              {pageTitles[route.page]}
            </Text>
          )}
          {samplePages.has(route.page) ? (
            <Badge tone="neutral">Sample data</Badge>
          ) : null}
        </>
      }
      center={null}
      end={
        <>
          <CoreIndicator />
          <IconButton
            size="sm"
            label="Search conversations and projects"
            icon={<SearchIcon />}
            onClick={() => setSearchOpen(true)}
          />
          <IconButton
            size="sm"
            label="Manage models"
            icon={<BoxIcon />}
            onClick={() => navigate({ page: "models" })}
          />
          <IconButton
            size="sm"
            label="Account and settings"
            icon={<UsersIcon />}
            onClick={() => setAccountOpen(true)}
          />
        </>
      }
    />
  );

  return (
    <ShellContext.Provider value={shell}>
      <AppShell
        rail={
          <NavRail
            label="Main navigation"
            items={rail}
            activeId={railIdFor(route)}
            onSelect={selectRail}
          />
        }
        sidebar={sidebarOpen ? <Sidebar /> : undefined}
        header={header}
        contentRef={contentRef}
      >
        {children}
      </AppShell>
      <QuickSearch
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        inputRef={searchRef}
      />
      <Dialog
        open={accountOpen}
        title="Account and settings"
        onClose={() => setAccountOpen(false)}
      >
        {accountError && <Notice tone="danger">{accountError}</Notice>}
        <Button
          variant="ghost"
          onClick={() => {
            setAccountOpen(false);
            navigate({ page: "models", tab: "providers" });
          }}
        >
          Provider accounts
        </Button>
        <Button
          variant="ghost"
          onClick={() => {
            setAccountOpen(false);
            navigate({ page: "settings" });
          }}
        >
          Preferences
        </Button>
        <Button
          variant="outline"
          icon={<LockIcon />}
          onClick={() =>
            void call("vault.lock")
              .then(() => window.location.reload())
              .catch(() =>
                setAccountError(
                  "The vault could not be locked. Stop active tasks and try again.",
                ),
              )
          }
        >
          Lock drive
        </Button>
      </Dialog>
    </ShellContext.Provider>
  );
}

/** Live drive and AiDEX Core state for the top bar. */
function CoreIndicator() {
  const { status, bridgeMissing, daemonRunning } = useAidexStatus();
  const [open, setOpen] = useState(false);
  const label = bridgeMissing
    ? "AiDEX unavailable"
    : !status
      ? "Checking drive"
      : status.drive.state !== "resolved"
        ? "No drive"
        : daemonRunning
          ? "Drive connected"
          : "AiDEX Core stopped";
  return (
    <>
      <IconButton
        size="sm"
        label={label}
        icon={
          <span
            className={`${styles.dot} ${daemonRunning ? styles.ready : styles.waiting}`}
          />
        }
        onClick={() => setOpen(true)}
      />
      <Dialog
        open={open}
        title="Drive connection"
        onClose={() => setOpen(false)}
      >
        <Stack gap={3}>
          <Text>{label}</Text>
          <Text tone="muted">
            {status?.drive.state === "resolved"
              ? status.drive.root
              : "No portable drive selected"}
          </Text>
        </Stack>
      </Dialog>
    </>
  );
}
