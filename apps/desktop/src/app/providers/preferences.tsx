import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { DEFAULT_ACCENT, applyAccent } from "@aidex/ui/themes";
import { modelOptions } from "../../lib/sample/models";
import { sampleWorkspaces } from "../../lib/sample/workspaces";

type Preferences = {
  /** Model used for new chats and agent runs; shared by the top bar and composers. */
  modelId: string;
  setModelId: (id: string) => void;
  accent: string;
  setAccent: (color: string) => void;
  /** Workspace that chats and agent runs act on by default. */
  workspaceId: string;
  setWorkspaceId: (id: string) => void;
  /** Prompt handed from Home to the chat or agent screen, consumed once on arrival. */
  pendingPrompt: string | null;
  setPendingPrompt: (prompt: string | null) => void;
};

const PreferencesContext = createContext<Preferences | null>(null);

/** Session preferences held in memory; nothing is written to the drive yet. */
export function PreferencesProvider({ children }: { children: ReactNode }) {
  const [modelId, setModelId] = useState<string>(modelOptions[0].value);
  const [accent, setAccent] = useState<string>(DEFAULT_ACCENT);
  const [workspaceId, setWorkspaceId] = useState<string>(sampleWorkspaces[0].id);
  const [pendingPrompt, setPendingPrompt] = useState<string | null>(null);

  useEffect(() => applyAccent(accent), [accent]);

  const value = useMemo(
    () => ({ modelId, setModelId, accent, setAccent, workspaceId, setWorkspaceId, pendingPrompt, setPendingPrompt }),
    [modelId, accent, workspaceId, pendingPrompt],
  );
  return <PreferencesContext.Provider value={value}>{children}</PreferencesContext.Provider>;
}

export function usePreferences(): Preferences {
  const preferences = useContext(PreferencesContext);
  if (!preferences) throw new Error("usePreferences must be used inside PreferencesProvider");
  return preferences;
}
