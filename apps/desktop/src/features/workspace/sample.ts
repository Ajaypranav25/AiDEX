// Sample data until AiDEX Core exposes workspace indexing, activity, and persistent memory.
import type { SampleWorkspace } from "../../lib/sample/workspaces";

export const locationLabel: Record<SampleWorkspace["location"], string> = {
  drive: "AiDEX drive",
  host: "This computer",
};

export type IndexStatus = {
  filesIndexed: number;
  filesTotal: number;
  chunks: number;
  lastIndexed: string;
};

const indexByWorkspace: Readonly<Record<string, IndexStatus>> = {
  "ai-dex": { filesIndexed: 1284, filesTotal: 1312, chunks: 9640, lastIndexed: "Today, 9:40 AM" },
  lattice: { filesIndexed: 412, filesTotal: 412, chunks: 2875, lastIndexed: "Yesterday, 2:01 PM" },
  tony: { filesIndexed: 96, filesTotal: 104, chunks: 610, lastIndexed: "3 days ago" },
  finapp: { filesIndexed: 738, filesTotal: 802, chunks: 5120, lastIndexed: "Today, 8:55 AM" },
  oddwriter: { filesIndexed: 221, filesTotal: 221, chunks: 1490, lastIndexed: "4 days ago" },
  z0d1ak: { filesIndexed: 0, filesTotal: 356, chunks: 0, lastIndexed: "Not indexed yet" },
  olympus: { filesIndexed: 188, filesTotal: 240, chunks: 1302, lastIndexed: "2 weeks ago" },
};

export const indexStatusFor = (workspaceId: string): IndexStatus =>
  indexByWorkspace[workspaceId] ?? { filesIndexed: 0, filesTotal: 0, chunks: 0, lastIndexed: "Not indexed yet" };

export const EMBEDDING_MODEL = "nomic-embed-text v1.5 · Local";

export const recentActivity = [
  { id: "a-tests", title: "Tests updated by an agent run", time: "2 hours ago" },
  { id: "a-validation", title: "Added route validation", time: "Yesterday" },
  { id: "a-indexed", title: "Project indexed for context", time: "Monday" },
] as const;

export type MemoryEntry = { id: string; title: string; source: string; updated: string };

export const sampleMemory: readonly MemoryEntry[] = [
  { id: "m-pnpm", title: "Use pnpm for installs and scripts, never npm", source: "Chat · Review API design", updated: "Today" },
  { id: "m-tests", title: "Run the test suite before proposing a commit", source: "Agent rule", updated: "Yesterday" },
  { id: "m-style", title: "Prefer small, focused files under 250 lines", source: "Chat · Design system discussion", updated: "Tuesday" },
  { id: "m-secrets", title: "Secrets live in the vault, not in .env files", source: "Added manually", updated: "Last week" },
];

/** Projects found on the AiDEX drive that are not workspaces yet. */
export const driveProjectOptions = [
  { value: "notes-vault", label: "notes-vault · AiDEX:/projects/notes-vault" },
  { value: "aidex-plugins", label: "aidex-plugins · AiDEX:/projects/aidex-plugins" },
  { value: "research-kit", label: "research-kit · AiDEX:/projects/research-kit" },
] as const;
