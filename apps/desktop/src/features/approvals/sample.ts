// Sample approval requests until AiDEX Core exposes the approval service (crates/approvals).
// Commands and effects mirror the broker's typed intents; nothing here is executed.

export type ApprovalRisk = "high" | "medium" | "low";

export type ApprovalRequest = {
  id: string;
  /** Human-readable intent, e.g. "Run shell command". */
  summary: string;
  command: string;
  /** What the action changes, in plain words. */
  effect: string;
  agent: string;
  workspaceId: string;
  risk: ApprovalRisk;
  /** Human-readable request time, e.g. "2 min ago". */
  requested: string;
};

export type ApprovalDecision = "approved" | "denied";

export type ApprovalRecord = ApprovalRequest & {
  decision: ApprovalDecision;
  rememberForSession: boolean;
  decided: string;
};

export const samplePendingApprovals: readonly ApprovalRequest[] = [
  {
    id: "apr-git-push",
    summary: "Push to a remote branch",
    command: "git push origin feature/auth",
    effect: "Uploads 3 local commits from feature/auth to the origin remote. Remote writes can't be undone from this workspace.",
    agent: "AiDEX Native",
    workspaceId: "finapp",
    risk: "high",
    requested: "1 min ago",
  },
  {
    id: "apr-rm-build",
    summary: "Delete a folder",
    command: "rm -rf build/",
    effect: "Removes the build output folder inside the workspace. Nothing outside the workspace mount is affected.",
    agent: "Codex",
    workspaceId: "ai-dex",
    risk: "medium",
    requested: "4 min ago",
  },
  {
    id: "apr-pnpm-install",
    summary: "Run shell command",
    command: "pnpm install --frozen-lockfile",
    effect: "Installs the exact versions pinned in pnpm-lock.yaml. Network access is limited to registry.npmjs.org.",
    agent: "AiDEX Native",
    workspaceId: "finapp",
    risk: "low",
    requested: "6 min ago",
  },
];

export const sampleApprovalHistory: readonly ApprovalRecord[] = [
  {
    id: "apr-ssh-config",
    summary: "Write outside the workspace",
    command: "echo 'Host *' >> ~/.ssh/config",
    effect: "Appends to the host SSH configuration, which is outside every workspace mount.",
    agent: "Claude Code",
    workspaceId: "z0d1ak",
    risk: "high",
    requested: "Today, 9:40 AM",
    decision: "denied",
    rememberForSession: false,
    decided: "Today, 9:41 AM",
  },
  {
    id: "apr-cargo-test",
    summary: "Run shell command",
    command: "cargo test --workspace",
    effect: "Builds and runs the workspace test suite inside the sandbox.",
    agent: "AiDEX Native",
    workspaceId: "ai-dex",
    risk: "low",
    requested: "Today, 9:12 AM",
    decision: "approved",
    rememberForSession: true,
    decided: "Today, 9:12 AM",
  },
];
