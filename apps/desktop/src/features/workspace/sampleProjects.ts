// Sample project contents until AiDEX Core exposes workspace files, git status, and toolchain detection.
// Each workspace borrows the profile of its primary language, so paths, diffs, and tools stay consistent.
import type { TreeNode } from "@aidex/ui/components";

export type ChangeStatus = "Modified" | "Added" | "Deleted";

export type ProjectProfile = {
  tree: readonly TreeNode[];
  /** Folder ids expanded when the tree first renders. */
  expanded: readonly string[];
  /** Source previews keyed by tree node id (the file's path). */
  snippets: Readonly<Record<string, { language: string; code: string }>>;
  changes: readonly { path: string; status: ChangeStatus; additions: number; deletions: number }[];
  commits: readonly { hash: string; message: string; author: string; time: string }[];
  toolchains: readonly { name: string; version: string; source: string; status: "Detected" | "Not running" }[];
  /** Question pre-filled in the retrieval test. */
  sampleQuery: string;
};

const file = (id: string, meta?: string): TreeNode => ({ id, name: id.split("/").pop() ?? id, kind: "file", meta });
const folder = (id: string, children: readonly TreeNode[]): TreeNode => ({
  id,
  name: id.split("/").pop() ?? id,
  kind: "folder",
  children,
});

const typescript: ProjectProfile = {
  tree: [
    folder("src", [
      folder("src/auth", [file("src/auth/session.ts", "M"), file("src/auth/routes.ts", "M")]),
      folder("src/lib", [file("src/lib/api.ts")]),
      file("src/main.tsx"),
    ]),
    folder("tests", [file("tests/auth.test.ts", "A")]),
    file("package.json"),
    file("tsconfig.json"),
    file("README.md"),
  ],
  expanded: ["src", "src/auth"],
  snippets: {
    "src/auth/session.ts": {
      language: "TypeScript",
      code: `import { signToken, verifyToken } from "../lib/api";

const SESSION_TTL_MS = 15 * 60 * 1000;

export async function refreshSession(token: string) {
  const claims = await verifyToken(token);
  if (claims.expiresAt < Date.now()) throw new Error("Session expired");
  return signToken({ ...claims, expiresAt: Date.now() + SESSION_TTL_MS });
}`,
    },
    "src/auth/routes.ts": {
      language: "TypeScript",
      code: `import { refreshSession } from "./session";

export const authRoutes = {
  "POST /auth/refresh": async (request: Request) => {
    const token = request.headers.get("authorization")?.replace("Bearer ", "");
    if (!token) return new Response("Missing token", { status: 401 });
    return Response.json({ token: await refreshSession(token) });
  },
};`,
    },
    "tests/auth.test.ts": {
      language: "TypeScript",
      code: `import { describe, expect, it } from "vitest";
import { refreshSession } from "../src/auth/session";

describe("refreshSession", () => {
  it("rejects an expired session token", async () => {
    await expect(refreshSession("expired-token")).rejects.toThrow("Session expired");
  });
});`,
    },
    "package.json": {
      language: "JSON",
      code: `{
  "name": "finapp",
  "private": true,
  "packageManager": "pnpm@9.12.0",
  "scripts": { "dev": "vite", "test": "vitest run", "typecheck": "tsc --noEmit" }
}`,
    },
  },
  changes: [
    { path: "src/auth/session.ts", status: "Modified", additions: 18, deletions: 6 },
    { path: "src/auth/routes.ts", status: "Modified", additions: 9, deletions: 2 },
    { path: "tests/auth.test.ts", status: "Added", additions: 24, deletions: 0 },
  ],
  commits: [
    { hash: "a1c94e2", message: "Add refresh token rotation", author: "Neeraj", time: "2 hours ago" },
    { hash: "7f0b3d1", message: "Validate auth route payloads", author: "Neeraj", time: "Yesterday" },
    { hash: "c52e8a9", message: "Move API client into lib", author: "Agent run", time: "Monday" },
  ],
  toolchains: [
    { name: "Node.js", version: "20.11", source: ".nvmrc", status: "Detected" },
    { name: "pnpm", version: "9.12", source: "package.json", status: "Detected" },
    { name: "TypeScript", version: "5.8", source: "tsconfig.json", status: "Detected" },
    { name: "Vitest", version: "2.1", source: "package.json", status: "Detected" },
  ],
  sampleQuery: "refresh session token",
};

const rust: ProjectProfile = {
  tree: [
    folder("crates", [
      folder("crates/core", [
        folder("crates/core/src", [file("crates/core/src/lib.rs"), file("crates/core/src/sandbox.rs", "M")]),
        file("crates/core/Cargo.toml"),
      ]),
    ]),
    folder("apps", [folder("apps/desktop", [file("apps/desktop/package.json")])]),
    file("Cargo.toml"),
    file("README.md", "M"),
  ],
  expanded: ["crates", "crates/core", "crates/core/src"],
  snippets: {
    "crates/core/src/sandbox.rs": {
      language: "Rust",
      code: `pub struct Sandbox {
    root: PathBuf,
    allow_network: bool,
}

impl Sandbox {
    /// Rejects any path that escapes the workspace root.
    pub fn resolve(&self, relative: &Path) -> Result<PathBuf, SandboxError> {
        let path = self.root.join(relative).canonicalize()?;
        if !path.starts_with(&self.root) {
            return Err(SandboxError::OutsideWorkspace(path));
        }
        Ok(path)
    }
}`,
    },
    "crates/core/src/lib.rs": {
      language: "Rust",
      code: `pub mod sandbox;

pub use sandbox::{Sandbox, SandboxError};

/// Version reported to the desktop shell during runtime detection.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");`,
    },
    "Cargo.toml": {
      language: "TOML",
      code: `[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
edition = "2021"
rust-version = "1.98"`,
    },
    "README.md": {
      language: "Markdown",
      code: `# AiDEX

A portable AI workspace that runs local models from an encrypted drive.

- \`crates/core\`: sandbox, model runtime, and agent harness
- \`apps/desktop\`: Tauri shell and screens`,
    },
  },
  changes: [
    { path: "crates/core/src/sandbox.rs", status: "Modified", additions: 31, deletions: 12 },
    { path: "README.md", status: "Modified", additions: 4, deletions: 1 },
  ],
  commits: [
    { hash: "e93a1f0", message: "Reject symlinks that escape the sandbox", author: "Agent run", time: "2 hours ago" },
    { hash: "4b7c2d8", message: "Add runtime detection to desktop shell", author: "Neeraj", time: "Yesterday" },
    { hash: "91d0e6a", message: "Bump workspace to Rust 1.98", author: "Neeraj", time: "Tuesday" },
  ],
  toolchains: [
    { name: "Rust", version: "1.98", source: "Cargo.toml", status: "Detected" },
    { name: "Cargo", version: "1.98", source: "PATH", status: "Detected" },
    { name: "Node.js", version: "20.11", source: ".nvmrc", status: "Detected" },
    { name: "pnpm", version: "9.12", source: "apps/desktop/package.json", status: "Detected" },
    { name: "TypeScript", version: "5.8", source: "apps/desktop/package.json", status: "Detected" },
  ],
  sampleQuery: "sandbox workspace path",
};

const go: ProjectProfile = {
  tree: [
    folder("cmd", [folder("cmd/lattice", [file("cmd/lattice/main.go")])]),
    folder("internal", [
      folder("internal/graph", [file("internal/graph/sync.go", "M"), file("internal/graph/sync_test.go", "A")]),
    ]),
    file("go.mod"),
    file("README.md"),
  ],
  expanded: ["internal", "internal/graph"],
  snippets: {
    "internal/graph/sync.go": {
      language: "Go",
      code: `// Sync merges remote edges into the local graph, keeping the newest version of each edge.
func (g *Graph) Sync(ctx context.Context, remote []Edge) (int, error) {
	merged := 0
	for _, edge := range remote {
		if err := ctx.Err(); err != nil {
			return merged, err
		}
		if g.upsert(edge) {
			merged++
		}
	}
	return merged, nil
}`,
    },
    "internal/graph/sync_test.go": {
      language: "Go",
      code: `func TestSyncKeepsNewestEdge(t *testing.T) {
	g := New()
	g.upsert(Edge{ID: "a", Version: 2})
	merged, _ := g.Sync(context.Background(), []Edge{{ID: "a", Version: 1}})
	if merged != 0 {
		t.Fatalf("merged = %d, want 0", merged)
	}
}`,
    },
    "go.mod": { language: "Go module", code: `module github.com/example/lattice\n\ngo 1.23` },
  },
  changes: [
    { path: "internal/graph/sync.go", status: "Modified", additions: 22, deletions: 9 },
    { path: "internal/graph/sync_test.go", status: "Added", additions: 17, deletions: 0 },
  ],
  commits: [
    { hash: "5d2f9b4", message: "Cancel graph sync on context timeout", author: "Neeraj", time: "Yesterday" },
    { hash: "0ac71e3", message: "Add edge versioning", author: "Agent run", time: "Tuesday" },
  ],
  toolchains: [
    { name: "Go", version: "1.23", source: "go.mod", status: "Detected" },
    { name: "golangci-lint", version: "1.61", source: "PATH", status: "Detected" },
    { name: "Docker", version: "27.2", source: "PATH", status: "Not running" },
  ],
  sampleQuery: "graph sync edges",
};

const python: ProjectProfile = {
  tree: [
    folder("src", [folder("src/app", [file("src/app/__init__.py"), file("src/app/evaluate.py", "M")])]),
    folder("tests", [file("tests/test_evaluate.py")]),
    file("pyproject.toml"),
    file("README.md"),
  ],
  expanded: ["src", "src/app"],
  snippets: {
    "src/app/evaluate.py": {
      language: "Python",
      code: `def score_answers(answers: list[str], expected: list[str]) -> float:
    """Share of answers that match the expected text, ignoring case and spacing."""
    if not expected:
        return 0.0
    matches = sum(a.strip().lower() == e.strip().lower() for a, e in zip(answers, expected))
    return matches / len(expected)`,
    },
    "tests/test_evaluate.py": {
      language: "Python",
      code: `from app.evaluate import score_answers


def test_score_ignores_case():
    assert score_answers(["Paris"], ["paris"]) == 1.0`,
    },
    "pyproject.toml": {
      language: "TOML",
      code: `[project]
name = "app"
requires-python = ">=3.12"

[tool.pytest.ini_options]
testpaths = ["tests"]`,
    },
  },
  changes: [{ path: "src/app/evaluate.py", status: "Modified", additions: 6, deletions: 3 }],
  commits: [
    { hash: "b8e40c7", message: "Normalize whitespace when scoring", author: "Neeraj", time: "3 days ago" },
    { hash: "62f1a9d", message: "Add evaluation tests", author: "Agent run", time: "Last week" },
  ],
  toolchains: [
    { name: "Python", version: "3.12", source: "pyproject.toml", status: "Detected" },
    { name: "uv", version: "0.4", source: "PATH", status: "Detected" },
    { name: "pytest", version: "8.3", source: "pyproject.toml", status: "Detected" },
  ],
  sampleQuery: "score answers",
};

const profileByLanguage: Readonly<Record<string, ProjectProfile>> = {
  TypeScript: typescript,
  Rust: rust,
  Go: go,
  Python: python,
};

/** The sample project contents for a workspace, chosen by its primary language. */
export const projectProfileFor = (languages: readonly string[]): ProjectProfile =>
  profileByLanguage[languages[0]] ?? typescript;
