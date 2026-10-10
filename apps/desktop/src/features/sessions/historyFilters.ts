// History filters live in the route so the page and its sidebar stay in sync:
// `tab` holds the session kind and `id` holds a workspace id.
import type { Route } from "../../lib/routes";
import { sampleSessions, type SampleSession } from "../../lib/sample/sessions";
import { sampleWorkspaces, type SampleWorkspace } from "../../lib/sample/workspaces";

export const kindFilters = ["all", "chat", "agent"] as const;
export type KindFilter = (typeof kindFilters)[number];

export const kindFilterOptions: readonly { value: KindFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "chat", label: "Chat" },
  { value: "agent", label: "Agent" },
];

export const kindLabel: Record<SampleSession["kind"], string> = { chat: "Chat", agent: "Agent run" };

/** The route's kind filter, falling back to "all" when absent or unknown. */
export function kindFilterOf(route: Route): KindFilter {
  return kindFilters.find((kind) => kind === route.tab) ?? "all";
}

/** The workspace the route filters by, or undefined for all workspaces (unknown ids count as none). */
export function workspaceFilterOf(route: Route): SampleWorkspace | undefined {
  return sampleWorkspaces.find((workspace) => workspace.id === route.id);
}

export function matchesKind(session: SampleSession, kind: KindFilter): boolean {
  return kind === "all" || session.kind === kind;
}

/** Sessions in the given workspace, or all sessions when no workspace is set. */
export function sessionsInWorkspace(workspaceId: string | undefined): readonly SampleSession[] {
  return workspaceId ? sampleSessions.filter((session) => session.workspaceId === workspaceId) : sampleSessions;
}

export function matchesQuery(session: SampleSession, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (!needle) return true;
  const workspaceName = sampleWorkspaces.find((workspace) => workspace.id === session.workspaceId)?.name ?? "";
  return `${session.title} ${workspaceName} ${session.model}`.toLowerCase().includes(needle);
}

export type DayGroup = { label: "Today" | "Yesterday" | "Earlier"; sessions: readonly SampleSession[] };

/** Groups sessions by the day prefix of their display time, keeping the input order within each group. */
export function groupByDay(sessions: readonly SampleSession[]): readonly DayGroup[] {
  const labelOf = (session: SampleSession): DayGroup["label"] => {
    if (session.time.startsWith("Today")) return "Today";
    if (session.time.startsWith("Yesterday")) return "Yesterday";
    return "Earlier";
  };
  const order: readonly DayGroup["label"][] = ["Today", "Yesterday", "Earlier"];
  return order
    .map((label) => ({ label, sessions: sessions.filter((session) => labelOf(session) === label) }))
    .filter((group) => group.sessions.length > 0);
}

export const sessionCountLabel = (count: number) => `${count} ${count === 1 ? "session" : "sessions"}`;

export const workspaceNameOf = (id: string) =>
  sampleWorkspaces.find((workspace) => workspace.id === id)?.name ?? id;
