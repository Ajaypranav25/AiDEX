// Sample agent run until AiDEX Core exposes live plans, tool calls, and subagent state.
import type { ActivityEntry, Step, TerminalLine } from "@aidex/ui/components";

export const DEFAULT_TASK = "Implement authentication";

export const planSteps: readonly Omit<Step, "status">[] = [
  { title: "Inspect existing authentication", description: "Read current auth routes and session handling" },
  { title: "Implement refresh flow", description: "Update session and token logic" },
  { title: "Write tests", description: "Cover expiry and refresh edge cases" },
  { title: "Verify changes", description: "Run checks and review the final diff" },
];

/** Plan steps with statuses for a run that has finished `completed` steps. */
export const stepsAt = (completed: number): Step[] =>
  planSteps.map((step, index) => ({
    ...step,
    status: index < completed ? "done" : index === completed ? "active" : "pending",
  }));

export const baseLog: readonly ActivityEntry[] = [
  { id: "log-1", time: "10:22:01", text: "Reading src/auth/session.ts" },
  { id: "log-2", time: "10:22:04", text: "Scanning authentication routes" },
  { id: "log-3", time: "10:22:07", text: "Identified refresh-token issue", tone: "accent" },
  { id: "log-4", time: "10:22:10", text: "Editing src/auth/session.ts" },
];

/** Log line added when the demo finishes each step after the first. */
export const advanceLog: readonly ActivityEntry["text"][] = [
  "Refresh flow updated in src/auth/session.ts",
  "Wrote expiry and refresh tests in tests/auth.test.ts",
  "Type check and 12 tests passed; diff ready for review",
];

export const permissions = [
  { label: "Read files", allowed: true },
  { label: "Edit with approval", allowed: true },
  { label: "Shell with approval", allowed: true },
  { label: "Network blocked", allowed: false },
] as const;

export type ToolCall = { id: string; tool: string; target: string; status: string; duration: string };

export const toolCalls: readonly ToolCall[] = [
  { id: "call-1", tool: "fs.read", target: "src/auth/session.ts", status: "Done", duration: "0.2 s" },
  { id: "call-2", tool: "fs.read", target: "src/auth/routes.ts", status: "Done", duration: "0.1 s" },
  { id: "call-3", tool: "fs.patch", target: "src/auth/session.ts", status: "Approved", duration: "1.4 s" },
  { id: "call-4", tool: "shell.exec", target: "pnpm test auth", status: "Running", duration: "8.9 s" },
];

export const terminalLines: readonly TerminalLine[] = [
  { kind: "command", text: "pnpm test auth" },
  { kind: "output", text: "RUN  v2.1.4  ~/Projects/FinApp" },
  { kind: "output", text: "✓ tests/auth.test.ts > refreshSession > rotates an expired token" },
  { kind: "output", text: "✓ tests/auth.test.ts > refreshSession > keeps a valid session" },
  { kind: "error", text: "✗ tests/auth.test.ts > routes > rejects a revoked token (retrying)" },
];

export type Subagent = {
  id: string;
  label: string;
  status: "done" | "running" | "pending" | "failed";
  detail: string;
  model: string;
  tools: string;
};

export const subagents: readonly Subagent[] = [
  { id: "analyze", label: "Analyze", status: "done", detail: "Read 6 files", model: "Gemma 4", tools: "fs.read, search" },
  { id: "implement", label: "Implement", status: "running", detail: "Editing src/auth/session.ts", model: "Qwen2.5 Coder 7B", tools: "fs.read, fs.patch" },
  { id: "verify", label: "Verify", status: "pending", detail: "Waits for Implement", model: "Qwen2.5 Coder 7B", tools: "shell.exec" },
  { id: "docs", label: "Docs", status: "pending", detail: "Update auth README", model: "Llama 3.2 3B", tools: "fs.patch" },
];

export const statusText: Record<Subagent["status"], string> = {
  done: "Done",
  running: "Running",
  pending: "Pending",
  failed: "Failed",
};

/** Workers listed in the execution sidebar, with how many tool calls each made in this run. */
export const runAgents = [
  { id: "main", label: "Main agent", calls: 1 },
  { id: "code", label: "Code agent", calls: 2 },
  { id: "test", label: "Test agent", calls: 1 },
] as const;
