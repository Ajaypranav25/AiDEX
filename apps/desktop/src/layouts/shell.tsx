import { createContext, useContext, type RefObject } from "react";

/** Shell actions that section sidebars and pages trigger without knowing the layout. */
export type Shell = {
  focusSearch: () => void;
  collapseSidebar: () => void;
  startNewSession: () => void;
  collapseButtonRef: RefObject<HTMLButtonElement | null>;
};

export const ShellContext = createContext<Shell | null>(null);

export function useShell(): Shell {
  const shell = useContext(ShellContext);
  if (!shell) throw new Error("useShell must be used inside MainLayout");
  return shell;
}
