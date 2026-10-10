import { useCallback, useState } from "react";
import { MainLayout } from "../layouts/MainLayout";
import { HomePage } from "../features/home/HomePage";
import { useCommandShortcut } from "../hooks/useCommandShortcut";
import type { PageId } from "../lib/navigation";

export function Workspace() {
  const [activePage, setActivePage] = useState<PageId>("home");
  // Remounting the home page gives each new session an empty composer; it takes focus on entry too.
  const [sessionKey, setSessionKey] = useState(0);

  const startNewSession = useCallback(() => {
    setActivePage("home");
    setSessionKey((key) => key + 1);
  }, []);

  useCommandShortcut("k", startNewSession);

  return (
    <MainLayout activePage={activePage} onNavigate={setActivePage} onNewSession={startNewSession}>
      <HomePage key={sessionKey} autoFocus />
    </MainLayout>
  );
}
