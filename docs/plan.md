# Implementation milestones

The repository follows the phases in the product specification. Milestones are capability goals, not promises that directories already implement them.

1. **Foundation** — repository/tooling, drive-relative paths, daemon lifecycle, versioned event/API contracts, desktop and CLI shells.
2. **Portable agent** — workspace access, native agent loop, brokered shell/file tools, durable sessions, local inference.
3. **Security** — vault, broker/policy/approvals, cleanup reporting, host restrictions.
4. **External agents** — CLI adapters and a stable adapter contract.
5. **Remote control** — device identity, pairing, encrypted control protocol, mobile approvals.
6. **Advanced harness** — delegation, task graph, checkpoints, verification, context and memory.
7. **Advanced sandbox and ecosystem** — kernel-backed isolation, network mediation, plugins, skills, MCP, model packs, signed updates.

The first usable vertical slice should establish one end-to-end path with explicit limits: discover a host, open a workspace, run a managed task through the broker, persist state on the drive, and recover it after restart. Security enforcement and portable-state claims must be verified before broadening the slice.

## Windows desktop release milestone

[Issue #55](https://github.com/NeerajCodz/AiDEX/issues/55) supersedes the older CLI-only roadmap boundary. The desktop now uses the daemon's cloud providers, ChatGPT OAuth accounts, managed llama.cpp inference, encrypted native conversations and agent runs, workspace services, and preferences. Normal Chat can operate without a project; Plan and Agent select explicit project context and permissions. The portable launcher opens the desktop from the drive's verified current bundle. Remote access displays **Coming soon**.

Protocol fixtures and local Windows tests cover these integrations. The portable Qwen2.5 Coder CPU runtime and project-free chat were exercised on H:, and the user verified chat and the compact interface. The verified bundle catalog/job library is implemented; exposing it through the installer wizard's catalog/progress/retry commands remains follow-up work. Live cloud-account inference, external-runtime account tasks, moving the updated application to a second laptop, and the complete installer/recovery acceptance matrix still require verification. This milestone remains open; these pending checks must not be treated as completed acceptance criteria.
