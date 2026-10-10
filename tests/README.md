# Tests

Organize future cross-application tests by level (unit, integration, and end-to-end) here; app- and crate-local tests belong with their owning packages. Those exist today: Rust tests run with `cargo test`, the TypeScript packages' Vitest suites with `bun run test`, and the desktop's Playwright smoke tests with `bun run --cwd apps/desktop test:e2e` (see `CLAUDE.md`). No cross-application tests exist yet, so none are implied by this directory.
