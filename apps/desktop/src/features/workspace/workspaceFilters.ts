import type { SampleWorkspace } from "../../lib/sample/workspaces";

export type WorkspaceFilter = "all" | "pinned" | "drive" | "host";

export const workspaceFilterOptions = [
  { value: "all", label: "All" },
  { value: "pinned", label: "Pinned" },
  { value: "drive", label: "On drive" },
  { value: "host", label: "This computer" },
] as const satisfies readonly { value: WorkspaceFilter; label: string }[];

/** The Workspaces page keeps its filter in `route.tab`, so the sidebar can reset it. */
export function filterOf(tab: string | undefined): WorkspaceFilter {
  return workspaceFilterOptions.find((option) => option.value === tab)?.value ?? "all";
}

export function matchesWorkspace(workspace: SampleWorkspace, filter: WorkspaceFilter, query: string): boolean {
  const inFilter =
    filter === "all" ||
    (filter === "pinned" && workspace.pinned) ||
    (filter !== "pinned" && workspace.location === filter);
  const text = `${workspace.name} ${workspace.path} ${workspace.languages.join(" ")}`.toLowerCase();
  return inFilter && text.includes(query.trim().toLowerCase());
}
