# Desktop features

Each folder owns one area of the desktop UI: its pages, its contextual sidebar, and its data. Screens are composed only from `@aidex/ui`. Screens whose backend exists read live data through `../hooks` (`useAidexStatus`, `useRpc`, `useBridgeQuery`); the rest keep labelled sample data (the shell badges every page in `samplePages`, `../lib/routes.ts`), and actions that need a missing backend say so with a "Preview" toast. See `../services/README.md` for wiring a new daemon-backed screen.

Live today: `launch` (drive, AiDEX Core, vault create and unlock, host and sandbox detection), the Home system status with the workspace registry and live events, Settings Diagnostics and Drive, and the Security sandbox capability report.

- `system`: shared live-status pieces: `CoreStatusNotice` (no drive, AiDEX Core stopped with Start, incompatible, outside the app), `RecentEvents` (`events.latest` + `events.replay`), `LiveEvents` (`events.subscribe`), and labels for host, sandbox, drive, and event data.
- `launch`: drive selection, drive and vault check, and host/runtime detection, shown before the workspace.
- `home`: the welcome dashboard, composer, and live system status.
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
