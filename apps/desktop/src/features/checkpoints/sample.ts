// Sample data until AiDEX Core exposes workspace checkpoints, snapshot metadata, and checkpoint diffs.
import type { DiffLine } from "@aidex/ui/components";

export type CheckpointChange = { path: string; summary: string };

export type SampleCheckpoint = {
  id: string;
  workspaceId: string;
  title: string;
  /** Display text, e.g. "Today, 10:24 AM". */
  time: string;
  description: string;
  taskState: string;
  model: string;
  /** Files that differ between this checkpoint and the current workspace. */
  changes: readonly CheckpointChange[];
  /** One representative file diff (checkpoint → current workspace) for the compare view. */
  diff: { fileName: string; additions: number; deletions: number; lines: readonly DiffLine[] };
};

const sessionDiff = {
  fileName: "src/auth/session.ts",
  additions: 3,
  deletions: 1,
  lines: [
    { kind: "context", oldLine: 12, newLine: 12, text: "export async function createSession(user: User) {" },
    { kind: "del", oldLine: 13, text: "  const token = sign(user.id);" },
    { kind: "add", newLine: 13, text: "  const token = sign(user.id, { expiresIn: SESSION_TTL });" },
    { kind: "add", newLine: 14, text: "  await sessions.store(token, user.id);" },
    { kind: "add", newLine: 15, text: "  audit.log(\"session.created\", { userId: user.id });" },
    { kind: "context", oldLine: 14, newLine: 16, text: "  return token;" },
    { kind: "context", oldLine: 15, newLine: 17, text: "}" },
  ],
} as const satisfies SampleCheckpoint["diff"];

const packageDiff = {
  fileName: "package.json",
  additions: 2,
  deletions: 2,
  lines: [
    { kind: "context", oldLine: 18, newLine: 18, text: "  \"dependencies\": {" },
    { kind: "del", oldLine: 19, text: "    \"jose\": \"^5.2.0\"," },
    { kind: "del", oldLine: 20, text: "    \"zod\": \"^3.22.4\"" },
    { kind: "add", newLine: 19, text: "    \"jose\": \"^5.9.6\"," },
    { kind: "add", newLine: 20, text: "    \"zod\": \"^3.23.8\"" },
    { kind: "context", oldLine: 21, newLine: 21, text: "  }," },
  ],
} as const satisfies SampleCheckpoint["diff"];

const docsDiff = {
  fileName: "docs/architecture.md",
  additions: 2,
  deletions: 0,
  lines: [
    { kind: "context", oldLine: 4, newLine: 4, text: "## Runtime" },
    { kind: "add", newLine: 5, text: "" },
    { kind: "add", newLine: 6, text: "Agents run inside the drive sandbox and request approval before edits." },
  ],
} as const satisfies SampleCheckpoint["diff"];

export const sampleCheckpoints: readonly SampleCheckpoint[] = [
  {
    id: "cp-auth-flow",
    workspaceId: "finapp",
    title: "Auth flow implemented",
    time: "Today, 10:24 AM",
    description: "Added authentication endpoints and unit tests.",
    taskState: "Saved after step 3 of 4",
    model: "Qwen2.5 Coder 7B",
    changes: [
      { path: "src/auth/session.ts", summary: "Modified · +3 −1" },
      { path: "src/auth/routes.ts", summary: "Modified · +12 −0" },
      { path: "tests/auth.test.ts", summary: "Added · +48" },
    ],
    diff: sessionDiff,
  },
  {
    id: "cp-before-deps",
    workspaceId: "finapp",
    title: "Before dependency update",
    time: "Today, 9:30 AM",
    description: "Saved before installing updated dependencies.",
    taskState: "Saved before step 2 of 4",
    model: "Qwen2.5 Coder 7B",
    changes: [
      { path: "package.json", summary: "Modified · +2 −2" },
      { path: "bun.lock", summary: "Modified · +86 −79" },
      { path: "src/auth/session.ts", summary: "Modified · +3 −1" },
      { path: "src/auth/routes.ts", summary: "Modified · +12 −0" },
      { path: "tests/auth.test.ts", summary: "Added · +48" },
    ],
    diff: packageDiff,
  },
  {
    id: "cp-login-view",
    workspaceId: "finapp",
    title: "Login view complete",
    time: "Yesterday, 6:40 PM",
    description: "Authentication UI and state management finished.",
    taskState: "Saved at the end of a chat",
    model: "Gemma 4",
    changes: [
      { path: "src/views/Login.tsx", summary: "Modified · +21 −6" },
      { path: "src/state/auth.ts", summary: "Modified · +9 −2" },
      { path: "src/auth/session.ts", summary: "Modified · +3 −1" },
    ],
    diff: sessionDiff,
  },
  {
    id: "cp-initial-import",
    workspaceId: "finapp",
    title: "Initial project import",
    time: "Monday, 11:15 AM",
    description: "Repository opened and indexed.",
    taskState: "No task in progress",
    model: "Gemma 4",
    changes: [],
    diff: { fileName: "No file changes", additions: 0, deletions: 0, lines: [] },
  },
  {
    id: "cp-sandbox-docs",
    workspaceId: "ai-dex",
    title: "Sandbox notes drafted",
    time: "Yesterday, 1:40 PM",
    description: "Documented the agent sandbox before the permission refactor.",
    taskState: "Saved after step 1 of 3",
    model: "Qwen2.5 Coder 7B",
    changes: [{ path: "docs/architecture.md", summary: "Modified · +2 −0" }],
    diff: docsDiff,
  },
  {
    id: "cp-design-tokens",
    workspaceId: "ai-dex",
    title: "Design tokens settled",
    time: "Tuesday, 4:10 PM",
    description: "Saved after agreeing on the color and spacing tokens.",
    taskState: "Saved at the end of a chat",
    model: "Gemma 4",
    changes: [{ path: "docs/architecture.md", summary: "Modified · +2 −0" }],
    diff: docsDiff,
  },
];

export const checkpointsFor = (workspaceId: string): readonly SampleCheckpoint[] =>
  sampleCheckpoints.filter((checkpoint) => checkpoint.workspaceId === workspaceId);

/** The checkpoint named by `id` in this workspace, falling back to the most recent one. */
export const selectedCheckpoint = (workspaceId: string, id: string | undefined): SampleCheckpoint | undefined => {
  const checkpoints = checkpointsFor(workspaceId);
  return checkpoints.find((checkpoint) => checkpoint.id === id) ?? checkpoints[0];
};
