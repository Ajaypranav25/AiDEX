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
bun run dev:cli -- daemon start --drive /path/to/drive
bun run dev:cli -- daemon status --drive /path/to/drive --json
bun run dev:cli -- events --workspace WORKSPACE_ID --drive /path/to/drive --follow
bun run dev:cli -- daemon stop --drive /path/to/drive
bun run dev:cli -- chat
bun run dev:cli -- run "Explain this repository"
bun run login:omp
```

`aidex init --drive PATH` requires an existing removable-drive root, verifies writability and at least 10 MiB free space, then idempotently creates only the directories listed by `aidex-drive.json`. It never formats a drive or replaces existing files. The root-level `aidex-drive.json` is the canonical schema-v2 JSON manifest. A valid schema-v1 manifest is migrated explicitly, with the original kept as `aidex-drive.v1.json`; malformed and unsupported manifests are reported and left untouched.

`aidex status [--drive PATH]` reads drive health without creating directories or repairing the manifest. Without `--drive`, status and doctor use `AIDEX_DRIVE_ROOT`, which a portable launcher can set. Status reports the resolved root, manifest schema version, writability, and available space; a missing or invalid manifest is an error. Doctor retains its host and Oh My Pi checks, adds actionable active-drive health output, and reports whether a daemon is running on the active drive (informational; it does not affect doctor's exit code).

`aidex daemon start|status|stop [--drive PATH]` manage the `aidexd` daemon for a drive (default `AIDEX_DRIVE_ROOT`). `start` launches the `aidexd` executable next to `aidex` (falling back to `aidexd` on `PATH`), recovering a stale endpoint first, and waits until the daemon answers; `stop` waits until the daemon has closed its connections and removed its endpoint. The daemon does not require an initialized drive yet. Every daemon-backed command connects through IPC contract v2 (`protocol/commands/README.md`); when no daemon is running it exits 3 with a hint to run `aidex daemon start --drive <path>`.

`aidex events --workspace ID [--after N] [--follow] [--drive PATH]` prints the workspace's durable events as newline-delimited JSON using `events.replay`, strictly after cursor `N`. With `--follow` it keeps polling from the last flushed event and reconnects after daemon restarts; an expired cursor is a permanent error (exit 8) rather than a silent gap.

`aidex` and `aidex chat` open Oh My Pi's interactive terminal UI. AiDEX passes `--approval-mode always-ask`, disables OMP extensions, MCP, LSP, skills, session persistence, and automatic title generation, and requests a limited tool set. OMP still owns its CLI tool lifecycle and approval implementation; this is not an AiDEX sandbox or policy boundary.

`aidex run` is headless, in-memory, and strictly restricts OMP SDK tools to `read`, `grep`, and `glob` (`restrictToolNames: true`). It disables MCP, LSP, extensions, custom tools, and skills. Writes and shell execution are not exposed in this mode.

Interactive chat uses the user's existing OMP provider credentials. Configure them with `bun run login:omp`; model access still requires a provider account or API key.

The source-tree `aidex run` adapter is `apps/cli/src/run.ts`. Packaged binaries must ship that script and set `AIDEX_RUNNER`; `AIDEX_BUN` can select a non-default Bun executable.
## Machine-readable output

The global `--json` flag makes `init`, `status`, `daemon start|status|stop`, `doctor`, and `agent list` print exactly one compact JSON document on stdout. Failures never write to stdout: with `--json` the error is a single JSON line on stderr,

```json
{"error":{"code":"daemon_unavailable","exit_code":3,"message":"daemon not running on /media/aidex: ...","hint":"start it with `aidex daemon start --drive /media/aidex`"}}
```

and without it a `aidex <command>: <message>` line plus an optional `hint:` line. `aidex events` always emits NDJSON events; `chat` and `run` are interactive/delegated and ignore `--json`. Clap usage errors keep clap's text format.

## Exit codes

Exit codes are stable (`crates/aidex-cli/src/exit.rs`, `Exit`); `error.code` in JSON output is the name column.

| Code | Name | Meaning |
| --- | --- | --- |
| 0 | `success` | The command succeeded. |
| 1 | `failure` | General failure (including an already-running daemon on `daemon start`, or doctor finding a problem). |
| 2 | `usage` | Invalid command line, or arguments the daemon rejected (`invalid_params`). |
| 3 | `daemon_unavailable` | No daemon, unreachable daemon, daemon stopping, or connection dropped. |
| 4 | `drive_error` | Drive missing, not removable, unwritable, or its manifest is missing/invalid. |
| 5 | `auth_failed` | Vault locked or authentication failed (including a rejected daemon capability). |
| 6 | `policy_denied` | Denied by policy or permissions. |
| 7 | `model_unavailable` | No usable model or provider. |
| 8 | `not_found` | The requested entity or history does not exist (e.g. an expired event cursor). |
| 9 | `cancelled` | Cancelled or interrupted. |
| 10 | `incompatible_version` | CLI and daemon speak different IPC contract versions, or the daemon lacks the method. |

`chat` and `run` map the delegated Oh My Pi process status onto this table: 0 stays 0, an interrupt (status 130 or a signal) is 9, any other status is 1. Daemon JSON-RPC error codes map to exit codes as listed in `protocol/commands/README.md`.
