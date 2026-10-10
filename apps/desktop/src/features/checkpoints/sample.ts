// Sample data until AiDEX Core exposes workspace checkpoints, snapshot metadata, and checkpoint diffs.
import type { CodeDiffSource } from "@aidex/ui/components";

export type CheckpointChange = { path: string; summary: string };

/** One file diff from the checkpoint to the current workspace: both versions, or a unified patch. */
export type CheckpointDiff = { fileName: string } & CodeDiffSource;

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
  /** One representative file diff for the compare view; absent when nothing changed. */
  diff?: CheckpointDiff;
};

const sessionDiff: CheckpointDiff = {
  fileName: "src/auth/session.ts",
  oldCode: `import { sign } from "./tokens";
import type { User } from "./types";

export async function createSession(user: User) {
  const token = sign(user.id);
  return token;
}
`,
  newCode: `import { audit } from "../audit";
import { SESSION_TTL, sessions } from "./store";
import { sign } from "./tokens";
import type { User } from "./types";

export async function createSession(user: User) {
  const token = sign(user.id, { expiresIn: SESSION_TTL });
  await sessions.store(token, user.id);
  audit.log("session.created", { userId: user.id });
  return token;
}
`,
};

// A git-style patch, as AiDEX Core will report it for files too large to send whole.
const packageDiff: CheckpointDiff = {
  fileName: "package.json",
  patch: `diff --git a/package.json b/package.json
--- a/package.json
+++ b/package.json
@@ -14,9 +14,9 @@
     "test": "vitest run",
     "typecheck": "tsc --noEmit"
   },
   "dependencies": {
-    "jose": "^5.2.0",
-    "zod": "^3.22.4"
+    "jose": "^5.9.6",
+    "zod": "^3.23.8"
   },
   "devDependencies": {
     "typescript": "~5.8.3"
`,
};

const docsDiff: CheckpointDiff = {
  fileName: "docs/architecture.md",
  oldCode: `# Architecture

AiDEX runs local models from an encrypted drive.

## Runtime
`,
  newCode: `# Architecture

AiDEX runs local models from an encrypted drive.

## Runtime

Agents run inside the drive sandbox and request approval before edits.
`,
};

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
      { path: "src/auth/session.ts", summary: "Modified · +5 −1" },
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
      { path: "src/auth/session.ts", summary: "Modified · +5 −1" },
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
      { path: "src/auth/session.ts", summary: "Modified · +5 −1" },
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
