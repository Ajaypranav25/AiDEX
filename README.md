# AiDEX

AiDEX is a portable AI development environment: its agent workflows, tools, models, encrypted state, and policies are designed to travel with removable storage while adapting to the host computer. This repository is in its CLI-first foundation phase: the Rust CLI and core crates are active work, the desktop and installer remain Tauri clients, and the full AiDEX-native agent runtime, drive provisioning, encryption, and remote control are still planned.

## Repository layout

- `apps/cli` — first-class `aidex` CLI, implemented in `crates/aidex-cli`; Oh My Pi supplies the initial agent runtime.
- `apps/desktop` — Tauri desktop client; retained while the shared Rust crates take priority.
- `apps/installer` — Tauri drive-setup preview; it does not yet provision a drive.
- `apps/mobile` — reserved future client; not implemented yet.
- `crates/` — active Rust workspace for the CLI, host, portable paths, sessions, storage, events, and daemon foundations.
- `packages/` — shared frontend packages; `packages/ui` (`@aidex/ui`) provides the design tokens and components the desktop screens are built from.
- `protocol/` — reserved versioned API-contract boundary; no generated client exists yet.
- `integrations/` — reserved metadata and definitions for adapters, MCP, skills, plugins, and providers.
- `portable/` — reserved distribution layouts, manifests, bootstrap, and packaging.
- `scripts/` — reserved development, build, packaging, signing, and release automation.
- `docs/` — product requirements, architecture, security constraints, and development workflow.

The directory structure separates real foundation code from planned product domains. A directory or interface is not evidence that the corresponding service is implemented; current capabilities and deferred areas are called out in the relevant architecture docs.

## Prerequisites

- Bun 1.3.14+
- Rust stable toolchain
- Tauri v2 platform prerequisites only when building the GUI apps

## Run the CLI

```sh
bun install
bun run dev:cli -- --help
bun run dev:cli -- init --drive /path/to/removable-drive
bun run dev:cli -- status --drive /path/to/removable-drive
bun run dev:cli -- doctor
```

`aidex init` creates only the directories declared by the root-level `aidex-drive.json` manifest. `aidex status` and `aidex doctor` inspect that same schema-v1 JSON manifest without repairing it. `aidex chat` starts Oh My Pi with interactive approvals; `aidex run` is non-interactive and read-only. See [CLI setup](apps/cli/README.md) for runtime setup and boundaries.

The Tauri clients remain available with `bun run dev:desktop` and `bun run dev:installer`.

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
