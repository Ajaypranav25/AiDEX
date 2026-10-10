# AiDEX

AiDEX is a portable AI development environment: its agent workflows, tools, models, encrypted state, and policies are designed to travel with removable storage while adapting to the host computer. This repository is at the foundation stage: it contains desktop and installer shells and a small Rust foundation; the full agent runtime, drive provisioning, encryption, and remote control remain planned.

## Repository layout

- `apps/desktop` — primary Tauri desktop application.
- `apps/installer` — Tauri drive-setup application; it does not yet provision a drive.
- `apps/cli` and `apps/mobile` — reserved future clients; neither is implemented yet.
- `crates/` — Rust workspace for portable paths, host capabilities, durable state, and the future core daemon.
- `packages/` and `protocol/` — reserved shared frontend and versioned API-contract boundaries; no generated client exists yet.
- `integrations/` — reserved metadata and definitions for adapters, MCP, skills, plugins, and providers.
- `portable/` — reserved distribution layouts, manifests, bootstrap, and packaging.
- `scripts/` — reserved development, build, packaging, signing, and release automation.
- `docs/` — product requirements, architecture, security constraints, and development workflow.

The directory structure separates real foundation code from planned product domains. A directory or interface is not evidence that the corresponding service is implemented; current capabilities and deferred areas are called out in the relevant architecture docs.

## Prerequisites

- Bun 1.3+
- Rust stable toolchain
- Tauri v2 platform prerequisites for the host OS

## Run the applications

```sh
bun install
bun run dev:desktop
bun run dev:installer
```

Both Tauri frontends use Next.js with Turbopack and export static assets for the webview. The installer is a read-only setup preview; it does not scan, encrypt, or provision drives.

## Rust workspace

```sh
cargo check --workspace
```

## Project guidance

- [Architecture and boundaries](docs/architecture.md)
- [Development workflow](docs/development.md)
- [Product scope and safety boundaries](docs/architecture.md#product-boundary)
- [Foundation requirements](docs/requirements.md)
- [Security constraints](docs/security.md)
- [Implementation plan](docs/plan.md)
- [Git contribution guidance](CONTRIBUTING.md)
