import type { ComponentType } from "react";
import { FolderIcon, GitBranchIcon, HistoryIcon, HomeIcon, MemoryIcon, TerminalIcon, type IconProps } from "@aidex/ui/icons";
import type { PageTab } from "../../lib/routes";

export type WorkspaceTab = PageTab<"workspace">;

/** Labels and icons for the workspace tabs, shared by the page's tab list and the sidebar. */
export const workspaceTabs: readonly { value: WorkspaceTab; label: string; Icon: ComponentType<IconProps> }[] = [
  { value: "overview", label: "Overview", Icon: HomeIcon },
  { value: "files", label: "Files", Icon: FolderIcon },
  { value: "git", label: "Git", Icon: GitBranchIcon },
  { value: "environment", label: "Environment", Icon: TerminalIcon },
  { value: "sessions", label: "Sessions", Icon: HistoryIcon },
  { value: "memory", label: "Context & memory", Icon: MemoryIcon },
];
