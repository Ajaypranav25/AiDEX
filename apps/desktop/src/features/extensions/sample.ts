// Sample data until AiDEX Core exposes installed skills, MCP servers, plugins, and the marketplace.
import type { ActivityEntry } from "@aidex/ui/components";

export type SkillCategory = "Code quality" | "Languages" | "Writing";

export type Skill = {
  id: string;
  name: string;
  description: string;
  category: SkillCategory;
  enabled: boolean;
  definition: string;
};

const skillFile = (name: string, description: string, steps: readonly string[]) =>
  [`---`, `name: ${name}`, `description: ${description}`, `---`, ``, `# ${name}`, ``, ...steps.map((step, i) => `${i + 1}. ${step}`)].join("\n");

export const skillCategories: readonly SkillCategory[] = ["Code quality", "Languages", "Writing"];

export const sampleSkills: readonly Skill[] = [
  {
    id: "code-review",
    name: "Code review",
    description: "Checks patches for bugs, risky changes, and missing tests",
    category: "Code quality",
    enabled: true,
    definition: skillFile("code-review", "Review a diff before it is applied", [
      "Read the full diff and the files it touches.",
      "Flag correctness, security, and error-handling issues first.",
      "Note missing tests and suggest the smallest fix for each finding.",
    ]),
  },
  {
    id: "test-writer",
    name: "Test writer",
    description: "Writes focused unit tests for the code under change",
    category: "Code quality",
    enabled: false,
    definition: skillFile("test-writer", "Add unit tests for changed code", [
      "Find the public behavior the change affects.",
      "Write one test per behavior using the project's existing framework.",
      "Run the tests and report failures with their output.",
    ]),
  },
  {
    id: "rust-development",
    name: "Rust development",
    description: "Cargo, clippy, and tests for Rust crates",
    category: "Languages",
    enabled: true,
    definition: skillFile("rust-development", "Build and verify Rust changes", [
      "Run `cargo check` after each edit.",
      "Fix every `cargo clippy` warning in touched files.",
      "Run `cargo test` for the affected crates before finishing.",
    ]),
  },
  {
    id: "typescript",
    name: "TypeScript",
    description: "Strict types, React patterns, and the project's lint rules",
    category: "Languages",
    enabled: true,
    definition: skillFile("typescript", "Write strict, idiomatic TypeScript", [
      "Keep `strict` clean; avoid `any` and non-null assertions.",
      "Prefer narrow union types over boolean flags.",
      "Run the type checker before finishing.",
    ]),
  },
  {
    id: "documentation",
    name: "Documentation",
    description: "Project documentation, READMEs, and specs",
    category: "Writing",
    enabled: true,
    definition: skillFile("documentation", "Keep docs accurate and concise", [
      "Update the README next to any changed module.",
      "Use sentence case and short, specific sentences.",
      "Link to source files instead of copying code.",
    ]),
  },
  {
    id: "release-notes",
    name: "Release notes",
    description: "Drafts user-facing notes from merged changes",
    category: "Writing",
    enabled: false,
    definition: skillFile("release-notes", "Summarize changes for users", [
      "Group merged changes by feature area.",
      "Describe what changed for the user, not the implementation.",
      "List breaking changes first.",
    ]),
  },
];

export type ServerStatus = "Connected" | "Needs authentication" | "Stopped";

export type McpServer = {
  id: string;
  name: string;
  description: string;
  transport: "stdio" | "HTTP";
  status: ServerStatus;
  endpoint: string;
  tools: readonly string[];
  auth: { status: string; required: boolean };
  log: readonly ActivityEntry[];
};

export const mcpServers: readonly McpServer[] = [
  {
    id: "filesystem",
    name: "Filesystem",
    description: "Local workspace file access",
    transport: "stdio",
    status: "Connected",
    endpoint: "npx -y @modelcontextprotocol/server-filesystem AiDEX:/workspaces/ai-dex",
    tools: ["read_file", "write_file", "list_directory", "search_files", "move_file"],
    auth: { status: "Not required", required: false },
    log: [
      { time: "10:42:03", text: "Started process (pid 18432)", tone: "success" },
      { time: "10:42:04", text: "Registered 5 tools" },
      { time: "11:05:17", text: "read_file src/lib/routes.ts · 4 ms" },
      { time: "11:05:19", text: "search_files \"tabOf\" · 38 ms" },
    ],
  },
  {
    id: "git",
    name: "Git",
    description: "Repository status, diffs, and history",
    transport: "stdio",
    status: "Connected",
    endpoint: "uvx mcp-server-git --repository AiDEX:/workspaces/ai-dex",
    tools: ["git_status", "git_diff", "git_log", "git_show"],
    auth: { status: "Not required", required: false },
    log: [
      { time: "10:42:03", text: "Started process (pid 18440)", tone: "success" },
      { time: "10:42:05", text: "Registered 4 tools" },
      { time: "11:02:51", text: "git_status · 12 ms" },
    ],
  },
  {
    id: "github",
    name: "GitHub",
    description: "Issues and pull requests",
    transport: "HTTP",
    status: "Needs authentication",
    endpoint: "https://api.githubcopilot.com/mcp/",
    tools: ["list_issues", "get_pull_request", "create_pull_request", "add_comment"],
    auth: { status: "Token missing from the vault", required: true },
    log: [
      { time: "10:42:06", text: "Connecting over HTTP" },
      { time: "10:42:07", text: "401 Unauthorized: no token configured", tone: "danger" },
    ],
  },
  {
    id: "postgres",
    name: "Postgres",
    description: "Read-only queries against a local database",
    transport: "stdio",
    status: "Stopped",
    endpoint: "npx -y @modelcontextprotocol/server-postgres postgresql://localhost/finapp_dev",
    tools: ["query"],
    auth: { status: "Connection string in the vault", required: false },
    log: [
      { time: "Yesterday 17:20", text: "Stopped by user" },
    ],
  },
];

export type Permission = "Files read" | "Files write" | "Network" | "Shell";

export type Plugin = {
  id: string;
  name: string;
  publisher: string;
  version: string;
  description: string;
  permissions: readonly Permission[];
  enabled?: boolean;
};

export const installedPlugins: readonly Plugin[] = [
  {
    id: "workspace-indexer",
    name: "Workspace indexer",
    publisher: "AiDEX",
    version: "1.4.2",
    description: "Builds the local search index used by chat and agents.",
    permissions: ["Files read"],
    enabled: true,
  },
  {
    id: "test-runner",
    name: "Test runner",
    publisher: "AiDEX",
    version: "0.9.0",
    description: "Detects the project's test command and reports results inline.",
    permissions: ["Files read", "Shell"],
    enabled: true,
  },
  {
    id: "docs-search",
    name: "Docs search",
    publisher: "Community",
    version: "2.1.0",
    description: "Looks up library documentation when the network is available.",
    permissions: ["Network"],
    enabled: false,
  },
  {
    id: "commit-helper",
    name: "Commit helper",
    publisher: "Northwind Labs",
    version: "0.4.1",
    description: "Drafts conventional commit messages from staged changes.",
    permissions: ["Files read", "Shell"],
    enabled: true,
  },
];

export const marketplacePlugins: readonly Plugin[] = [
  {
    id: "issue-sync",
    name: "Issue tracker sync",
    publisher: "Northwind Labs",
    version: "1.2.0",
    description: "Links sessions to issues and posts summaries when a task is done.",
    permissions: ["Network"],
  },
  {
    id: "container-tools",
    name: "Container tools",
    publisher: "Community",
    version: "0.8.3",
    description: "Builds and runs project containers from the agent sandbox.",
    permissions: ["Files read", "Shell", "Network"],
  },
  {
    id: "notebook-viewer",
    name: "Notebook viewer",
    publisher: "AiDEX",
    version: "1.0.1",
    description: "Renders Jupyter notebooks in the file viewer.",
    permissions: ["Files read"],
  },
  {
    id: "diagram-renderer",
    name: "Diagram renderer",
    publisher: "Community",
    version: "2.0.0",
    description: "Turns Mermaid and PlantUML blocks into diagrams in chat.",
    permissions: ["Files read"],
  },
  {
    id: "lint-bridge",
    name: "Lint bridge",
    publisher: "Community",
    version: "0.6.0",
    description: "Runs the project's linters and feeds findings to agents.",
    permissions: ["Files read", "Shell"],
  },
  {
    id: "changelog-writer",
    name: "Changelog writer",
    publisher: "Northwind Labs",
    version: "1.1.4",
    description: "Updates CHANGELOG.md from merged changes.",
    permissions: ["Files read", "Files write"],
  },
];

/** Sidebar shortcuts to commonly used extensions; each opens its tab with the item selected. */
export const extensionShortcuts = [
  { id: "code-review", label: "Code review", tab: "skills" },
  { id: "filesystem", label: "Filesystem", tab: "mcp" },
  { id: "git", label: "Git", tab: "mcp" },
  { id: "workspace-indexer", label: "Workspace indexer", tab: "plugins" },
] as const;
