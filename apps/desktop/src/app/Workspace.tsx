import { useCallback, useState } from "react";
import { HomePage } from "../features/home/HomePage";
import { useCommandShortcut } from "../hooks/useCommandShortcut";
import { MainLayout } from "../layouts/MainLayout";
import { screenKeyOf } from "../lib/routes";
import { NavigationProvider, useNavigation } from "./providers/navigation";
import { PreferencesProvider, usePreferences } from "./providers/preferences";
import { ToastProvider } from "./providers/toast";
import { homeSidebar, screens } from "./registry";

/** The signed-in workspace: providers plus the route-driven shell. */
export function Workspace() {
  return (
    <NavigationProvider initialRoute={{ page: "chat" }}>
      <PreferencesProvider>
        <ToastProvider>
          <WorkspaceScreens />
        </ToastProvider>
      </PreferencesProvider>
    </NavigationProvider>
  );
}

function WorkspaceScreens() {
  const { route, navigate } = useNavigation();
  const { setPendingPrompt, setPendingWorkspaceId } = usePreferences();
  // A new chat resets its draft and optional project context.
  const [sessionKey, setSessionKey] = useState(0);

  const startNewSession = useCallback(() => {
    setPendingPrompt(null);
    setPendingWorkspaceId(null);
    navigate({ page: "chat" });
    setSessionKey((key) => key + 1);
  }, [navigate, setPendingPrompt, setPendingWorkspaceId]);

  useCommandShortcut("k", startNewSession);

  if (route.page === "home") {
    return (
      <MainLayout Sidebar={homeSidebar} onNewSession={startNewSession}>
        <HomePage key={sessionKey} autoFocus />
      </MainLayout>
    );
  }

  const { Page, Sidebar } = screens[route.page];
  return (
    <MainLayout Sidebar={Sidebar} onNewSession={startNewSession}>
      {/* Keyed by screen so local state resets when the same page opens another workspace, agent, or model. */}
      <Page
        key={`${screenKeyOf(route)}:${route.page === "chat" && !route.id ? sessionKey : 0}`}
      />
    </MainLayout>
  );
}
