# AiDEX

AiDEX is a portable AI development environment: its agent workflows, tools, models, encrypted state, and policies are designed to travel with removable storage while adapting to the host computer. This repository is in its CLI-first foundation phase: the Rust CLI and core crates are active work, the desktop and installer remain Tauri clients, and the full AiDEX-native agent runtime, drive provisioning, encryption, and remote control are still planned.

## Repository layout

- `apps/cli` — first-class `aidex` CLI, implemented in `crates/aidex-cli`; Oh My Pi supplies the initial agent runtime.
- `apps/desktop` — Tauri desktop client; retained while the shared Rust crates take priority.
- `apps/installer` — Tauri drive-setup preview; it does not yet provision a drive.
- `apps/mobile` — reserved future client; not implemented yet.
- `crates/` — active Rust workspace for the CLI, host, portable paths, sessions, storage, events, workspaces, local inference, and daemon foundations.
- `packages/` — shared frontend packages; `packages/ui` (`@aidex/ui`) provides the design tokens and components the desktop screens are built from.
- `protocol/` — reserved versioned API-contract boundary; no generated client exists yet.
- `integrations/` — reserved metadata and definitions for adapters, MCP, skills, plugins, and providers.
- `portable/` — drive layout and portable bundle packaging (`portable/packaging`), plus reserved bootstrap, manifest, model-pack, and toolchain areas.
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

`aidex init` creates only the directories declared by the root-level `aidex-drive.json` manifest. `aidex status` and `aidex doctor` inspect that same schema-v2 JSON manifest without repairing it; `aidex init` explicitly migrates a schema-v1 manifest and keeps the original as `aidex-drive.v1.json`. `aidex chat` starts Oh My Pi with interactive approvals; `aidex run` is non-interactive and read-only. See [CLI setup](apps/cli/README.md) for runtime setup and boundaries.

The Tauri clients remain available with `bun run dev:desktop` and `bun run dev:installer`.

## Portable bundle

Build a self-contained bundle (no Rust, Bun, or Node needed on the machine that runs it), install it onto an initialized drive, and start AiDEX from the drive root:

```sh
cargo run --locked -p aidex-packager -- build --target windows-x64 --out dist
aidex init --drive H:\
cargo run -p aidex-packager -- install --bundle dist/aidex-0.1.0-windows-x64.zip --drive H:\
H:\AiDEX.exe doctor          # Windows; on Linux and macOS: /media/<drive>/aidex doctor
```

The launcher verifies the drive manifest and the bundle's binaries against `bundle.json` before running the CLI, and passes the drive root to it; nothing is installed on the host. Windows x64, Linux x64, and macOS arm64 bundles are built and smoke-tested in CI; other targets are not supported yet. Layout, manifest format, verification limits, and the support matrix: [portable packaging](portable/packaging/README.md).

## Local models

AiDEX runs GGUF models itself through a pinned, hash-verified llama.cpp runtime stored on the drive (no third-party agent, and no network access once the files are on the drive):

```sh
aidex daemon start --drive H:\
aidex runtime install llama-b11205-bin-win-cpu-x64.zip      # pinned in inference/runtimes.toml
aidex model import qwen2.5-coder-0.5b-instruct-q4_k_m.gguf --license Apache-2.0 --source https://huggingface.co/Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF
aidex runtime plan qwen2.5-coder-0.5b-instruct-q4_k_m --probe-gpu
aidex model run qwen2.5-coder-0.5b-instruct-q4_k_m "Write a Rust function that reverses a string."
```

Windows x64 is validated; other platforms are not yet (their `.tar.gz` runtimes cannot be installed by this build). Imported models are listed as `local/<id>` by `aidex model list` and work with `aidex offline on`. See [`crates/inference`](crates/inference/README.md) and [`inference/`](inference/README.md) for verification, licensing, and platform details.

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
