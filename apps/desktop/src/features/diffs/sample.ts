// Sample patch and verification results until AiDEX Core exposes workspace diffs and test runs.
import type { DiffLine } from "@aidex/ui/components";

export type ChangedFile = { id: string; path: string; additions: number; deletions: number; lines: readonly DiffLine[] };

export const changedFiles: readonly ChangedFile[] = [
  {
    id: "session",
    path: "src/auth/session.ts",
    additions: 18,
    deletions: 4,
    lines: [
      { kind: "context", oldLine: 12, newLine: 12, text: "export async function refreshSession(token: string) {" },
      { kind: "context", oldLine: 13, newLine: 13, text: "  const session = await store.get(token);" },
      { kind: "del", oldLine: 14, text: "  if (!session) return null;" },
      { kind: "add", newLine: 14, text: "  if (!session || session.expired) {" },
      { kind: "add", newLine: 15, text: "    return await rotateRefreshToken(token);" },
      { kind: "add", newLine: 16, text: "  }" },
      { kind: "context", oldLine: 15, newLine: 17, text: "  return session;" },
      { kind: "context", oldLine: 16, newLine: 18, text: "}" },
    ],
  },
  {
    id: "routes",
    path: "src/auth/routes.ts",
    additions: 9,
    deletions: 7,
    lines: [
      { kind: "context", oldLine: 30, newLine: 30, text: 'router.post("/auth/refresh", async (req, res) => {' },
      { kind: "del", oldLine: 31, text: "  const session = await refreshSession(req.body.token);" },
      { kind: "del", oldLine: 32, text: "  if (!session) return res.status(401).end();" },
      { kind: "add", newLine: 31, text: "  const token = parseRefreshToken(req);" },
      { kind: "add", newLine: 32, text: "  const session = await refreshSession(token);" },
      { kind: "add", newLine: 33, text: '  if (!session) return res.status(401).json({ error: "session_expired" });' },
      { kind: "context", oldLine: 33, newLine: 34, text: "  res.json(session);" },
      { kind: "context", oldLine: 34, newLine: 35, text: "});" },
    ],
  },
  {
    id: "tests",
    path: "tests/auth.test.ts",
    additions: 15,
    deletions: 0,
    lines: [
      { kind: "add", newLine: 41, text: 'describe("refreshSession", () => {' },
      { kind: "add", newLine: 42, text: '  it("rotates an expired token", async () => {' },
      { kind: "add", newLine: 43, text: "    const next = await refreshSession(expiredToken);" },
      { kind: "add", newLine: 44, text: "    expect(next?.token).not.toBe(expiredToken);" },
      { kind: "add", newLine: 45, text: "  });" },
      { kind: "add", newLine: 46, text: "});" },
    ],
  },
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
