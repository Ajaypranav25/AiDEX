# AiDEX CLI

`aidex` is the first active AiDEX client. Its Rust command surface lives in `crates/aidex-cli`; the desktop and installer Tauri clients remain in place while the Rust foundation stays the primary implementation focus.

## Agent runtime

The interactive command delegates to Oh My Pi's documented `omp` CLI so its terminal UI and approval dialogs stay upstream-owned. The headless `aidex run` command uses the pinned TypeScript SDK in `src/run.ts` and its explicit restricted-session API. AiDEX does not fork or import Oh My Pi's internal Rust crates: upstream documents those crates as implementation details behind its package.

Oh My Pi was selected over the original [Pi monorepo](https://github.com/badlogic/pi-mono) because its documented CLI and SDK support Bun 1.3.14+, matching this workspace. The Pi CLI package currently declares Node.js `>=22.19.0`; both projects are MIT-licensed. AiDEX consumes Oh My Pi through its supported runtime/package interfaces rather than importing its private Rust crates.

The dependency is pinned to `@oh-my-pi/pi-coding-agent` `18.8.7`, installed by Bun. Upstream references:

- [Oh My Pi repository](https://github.com/can1357/oh-my-pi)
- [CLI reference](https://github.com/can1357/oh-my-pi/blob/main/docs/cli-reference.md)
- [Approval modes and safety](https://github.com/can1357/oh-my-pi/blob/main/docs/approval-mode.md)
- [Native crate boundary](https://github.com/can1357/oh-my-pi/blob/main/docs/native-crates.md)
- [SDK integration](https://github.com/can1357/oh-my-pi/blob/main/docs/sdk.md)
- [Package metadata](https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/package.json)

## Commands

```sh
bun install
bun run dev:cli -- --help
bun run dev:cli -- init --drive /path/to/removable-drive
bun run dev:cli -- status --drive /path/to/removable-drive
bun run dev:cli -- doctor
bun run dev:cli -- agent list
bun run dev:cli -- chat
bun run dev:cli -- run "Explain this repository"
bun run login:omp
```

`aidex init --drive PATH` requires an existing removable-drive root, verifies writability and at least 10 MiB free space, then idempotently creates only the directories listed by `aidex-drive.json`. It never formats a drive or replaces existing files. The root-level `aidex-drive.json` is the canonical schema-v1 JSON manifest; malformed and unsupported manifests are reported and left untouched.

`aidex status [--drive PATH]` reads drive health without creating directories or repairing the manifest. Without `--drive`, status and doctor use `AIDEX_DRIVE_ROOT`, which a portable launcher can set. Status reports the resolved root, manifest schema version, writability, and available space; a missing or invalid manifest is an error. Doctor retains its host and Oh My Pi checks and adds actionable active-drive health output.

`aidex` and `aidex chat` open Oh My Pi's interactive terminal UI. AiDEX passes `--approval-mode always-ask`, disables OMP extensions, MCP, LSP, skills, session persistence, and automatic title generation, and requests a limited tool set. OMP still owns its CLI tool lifecycle and approval implementation; this is not an AiDEX sandbox or policy boundary.

`aidex run` is headless, in-memory, and strictly restricts OMP SDK tools to `read`, `grep`, and `glob` (`restrictToolNames: true`). It disables MCP, LSP, extensions, custom tools, and skills. Writes and shell execution are not exposed in this mode.

Interactive chat uses the user's existing OMP provider credentials. Configure them with `bun run login:omp`; model access still requires a provider account or API key.

The source-tree `aidex run` adapter is `apps/cli/src/run.ts`. Packaged binaries must ship that script and set `AIDEX_RUNNER`; `AIDEX_BUN` can select a non-default Bun executable.