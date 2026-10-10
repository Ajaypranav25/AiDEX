// Sample patch and verification results until AiDEX Core exposes workspace diffs and test runs.
import { getDiffStats } from "@aidex/ui/components";

export type ChangedFile = {
  id: string;
  path: string;
  /** File contents before the agent's edit; empty for new files. */
  oldCode: string;
  newCode: string;
  additions: number;
  deletions: number;
};

const sessionBefore = `import { store } from "./store";
import type { Session } from "./types";

const SESSION_TTL_MS = 15 * 60 * 1000;

export function isExpired(session: Session, now = Date.now()): boolean {
  return now - session.issuedAt > SESSION_TTL_MS;
}

export async function refreshSession(token: string) {
  const session = await store.get(token);
  if (!session) return null;
  return session;
}
`;

const sessionAfter = `import { store } from "./store";
import { signRefreshToken } from "./tokens";
import type { Session } from "./types";

const SESSION_TTL_MS = 15 * 60 * 1000;

export function isExpired(session: Session, now = Date.now()): boolean {
  return now - session.issuedAt > SESSION_TTL_MS;
}

/** Issues a new refresh token and drops the old one so it cannot be replayed. */
async function rotateRefreshToken(token: string): Promise<Session | null> {
  const previous = await store.get(token);
  if (!previous) return null;
  const next = { ...previous, token: signRefreshToken(previous.userId), issuedAt: Date.now() };
  await store.replace(token, next);
  return next;
}

export async function refreshSession(token: string) {
  const session = await store.get(token);
  if (!session || isExpired(session)) {
    return await rotateRefreshToken(token);
  }
  return session;
}
`;

const routesBefore = `import { Router } from "express";
import { refreshSession } from "./session";

export const router = Router();

router.post("/auth/refresh", async (req, res) => {
  const session = await refreshSession(req.body.token);
  if (!session) return res.status(401).end();
  res.json(session);
});
`;

const routesAfter = `import { Router, type Request } from "express";
import { refreshSession } from "./session";

export const router = Router();

function parseRefreshToken(req: Request): string {
  const token = req.cookies?.refresh_token ?? req.body?.token;
  if (typeof token !== "string" || token.length === 0) {
    throw new TypeError("Missing refresh token");
  }
  return token;
}

router.post("/auth/refresh", async (req, res) => {
  const token = parseRefreshToken(req);
  const session = await refreshSession(token);
  if (!session) return res.status(401).json({ error: "session_expired" });
  res.json(session);
});
`;

const testsAfter = `import { describe, expect, it } from "vitest";
import { refreshSession } from "../src/auth/session";
import { seedSession } from "./fixtures";

describe("refreshSession", () => {
  it("returns a live session unchanged", async () => {
    const live = await seedSession({ ageMinutes: 1 });
    const result = await refreshSession(live.token);
    expect(result?.token).toBe(live.token);
  });

  it("rotates an expired token", async () => {
    const expired = await seedSession({ ageMinutes: 30 });
    const next = await refreshSession(expired.token);
    expect(next?.token).not.toBe(expired.token);
  });
});
`;

const changedFile = (id: string, path: string, oldCode: string, newCode: string): ChangedFile => ({
  id,
  path,
  oldCode,
  newCode,
  ...getDiffStats(path, { oldCode, newCode }),
});

export const changedFiles: readonly ChangedFile[] = [
  changedFile("session", "src/auth/session.ts", sessionBefore, sessionAfter),
  changedFile("routes", "src/auth/routes.ts", routesBefore, routesAfter),
  changedFile("tests", "tests/auth.test.ts", "", testsAfter),
];

export const fileById = (id: string | undefined): ChangedFile =>
  changedFiles.find((file) => file.id === id) ?? changedFiles[0];

export const totals = changedFiles.reduce(
  (sum, file) => ({ additions: sum.additions + file.additions, deletions: sum.deletions + file.deletions }),
  { additions: 0, deletions: 0 },
);

export type CheckResult = { id: string; name: string; result: string; detail: string; outcome: "passed" | "warning" | "failed" };

export const verification: readonly CheckResult[] = [
  { id: "unit", name: "Unit tests", result: "8 passed", detail: "vitest · tests/auth.test.ts", outcome: "passed" },
  { id: "integration", name: "Integration tests", result: "4 passed", detail: "vitest · tests/routes.int.test.ts", outcome: "passed" },
  { id: "types", name: "Type check", result: "Passed", detail: "tsc --noEmit", outcome: "passed" },
  { id: "lint", name: "Lint", result: "1 warning", detail: "Unused import in src/auth/routes.ts", outcome: "warning" },
];

export const RUN_TESTS_MESSAGE =
  "Preview: tests will run in the workspace sandbox when AiDEX Core is connected. Showing the last sample results.";
