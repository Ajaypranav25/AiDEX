import { useCallback, useState } from "react";
import { HomePage } from "../features/home/HomePage";
import { useCommandShortcut } from "../hooks/useCommandShortcut";
import { MainLayout } from "../layouts/MainLayout";
import { NavigationProvider, useNavigation } from "./providers/navigation";
import { PreferencesProvider } from "./providers/preferences";
import { ToastProvider } from "./providers/toast";
import { homeSidebar, screens } from "./registry";

/** The signed-in workspace: providers plus the route-driven shell. */
export function Workspace() {
  return (
    <NavigationProvider initialRoute={{ page: "home" }}>
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
  // Remounting the home page gives each new session an empty composer; it takes focus on entry too.
  const [sessionKey, setSessionKey] = useState(0);

  const startNewSession = useCallback(() => {
    navigate({ page: "home" });
    setSessionKey((key) => key + 1);
  }, [navigate]);

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
      {/* Keyed by record so local state resets when the same page opens another workspace, agent, or model. */}
      <Page key={`${route.page}:${route.id ?? ""}`} />
    </MainLayout>
  );
}
