# Desktop features

Each folder owns one area of the desktop UI: its pages, its contextual sidebar, and its sample data. Screens are composed only from `@aidex/ui` and are UI previews: no backend calls or mock runtime exist yet, and actions that need AiDEX Core say so with a "Preview" toast. Replace sample data with typed API contracts once each Core endpoint is defined.

- `launch`: drive unlock and host/runtime detection, shown before the workspace.
- `home`: the welcome dashboard and composer.
- `workspace`: workspace list, add or import, overview (files, git, environment, sessions), and context and memory.
- `chat`: conversations with attachments, mode switch, and model picker.
- `agent`: agent runs (plan, activity, docked instructions) and the execution and task graph.
- `diffs`: change review with per-file decisions and verification results.
- `sessions`: session history with kind and workspace filters.
- `checkpoints`: checkpoint timeline with resume, restore, branch, and compare.
- `agent-center`: built-in agents, external runtimes, and per-agent configuration.
- `model-manager`: local models, provider accounts, and model details with benchmarks.
- `extensions`: skills, MCP servers, and plugins.
- `security`: security overview, vault and secrets, and sandbox and policies.
- `approvals`: pending and past approvals of sensitive actions.
- `remote-sessions`: device pairing and active remote sessions.
- `settings`: appearance, shortcuts, updates, diagnostics, and drive health and cleanup.
- `editor`, `tasks`, `terminal`: reserved for later screens.
