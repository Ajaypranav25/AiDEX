# Foundation requirements

These requirements capture the first repository milestone from the technical specification.

1. Desktop and installer are separate Tauri applications.
2. Portable path handling rejects absolute paths and parent traversal before resolving user-controlled relative paths.
3. The core daemon and frontends are separate processes/contracts; Tauri command handlers remain thin.
4. Durable session and configuration state must be drive-relative and restart-safe before claiming portability.
5. Sensitive actions must eventually be brokered and policy-checked; UI scaffolding does not grant execution capability.
6. Local model runtime and third-party agents are optional integrations, not implicit assumptions about installed binaries.
7. Remote control must be end-to-end protected and independently permission-scoped.
8. AiDEX must state when features are planned or unavailable and must not report simulated setup success.

See `docs/plan.md` for the phase sequence and `docs/architecture.md` for boundaries.