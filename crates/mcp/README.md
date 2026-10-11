# aidex-mcp

MCP (Model Context Protocol) server management for AiDEX (issue #24): per-server
configuration on the drive, `.mcp.json` import, an MCP client over stdio and
Streamable HTTP, and a supervisor that launches servers in the sandbox. The
daemon serves it as `mcp.*` (see `protocol/commands/README.md`) and the CLI as
`aidex mcp`. Native Agent runs offer the tools of enabled servers (started on
demand) as `mcp__<server>__<tool>` (see `crates/harness/README.md`); enabled,
trusted plugins contribute their `.mcp.json` servers as DISABLED entries with id
`<plugin>-<server>` and `source = { kind = "plugin", owner = "<plugin>" }`,
removed again when the plugin is disabled, uninstalled, or loses its trust.

## Configuration

One closed-schema TOML file per server, `DrivePaths::mcp()/<id>.toml`
(`McpConfigStore`, written atomically, links refused, 64 KiB cap, at most 256
servers). Unknown fields are rejected, so no field can carry a secret value.

```toml
version = 1
id = "github"                 # [a-z0-9][a-z0-9_-]{0,63}; also the vault consumer mcp:<id>
name = "GitHub"
transport = "stdio"           # or "http"
command = "npx"               # stdio only
args = ["-y", "@modelcontextprotocol/server-github"]
env_allowlist = ["NODE_OPTIONS"]   # parent variables copied in; credential-like names refused
enabled = false               # disabled until the user enables it
workspaces = []               # workspace ids it is offered in; empty = all
auto_approve_tools = ["search_repositories"]

[[env_from_secrets]]          # stdio: environment variable <- vault secret
env = "GITHUB_PERSONAL_ACCESS_TOKEN"
secret_name = "github-token"

# http servers instead: url = "https://mcp.example.com/mcp" (https, or http on
# loopback; no user info, query, or fragment) and
# [[headers_from_secrets]] header = "Authorization", secret_name = "..."
# (the vault value is the whole header value, e.g. "Bearer <token>").
```

Validation refuses command lines and URLs that look like they contain a
credential (anything `aidex_events::redact_text` would hide): secrets belong in
the vault, stored with consumer `mcp:<id>`. Imported entries also carry
`[source] kind = "workspace" | "plugin"` and its owner.

`McpConfigStore::import_mcp_json(path, source)` parses a Claude Code style
`.mcp.json` (`{"mcpServers": {name: {command, args, env, url, type, headers}}}`,
256 KiB cap, links refused) into DISABLED configurations without writing
anything. Environment and header *values* are dropped; each name becomes a
mapping to a suggested secret name (`mcp-<id>-<name>`) the user fills in.
Workspace imports are offered only in that workspace. Entries that cannot
become valid configurations are returned with an error.

## Client

`McpClient` speaks MCP JSON-RPC 2.0:

- stdio: newline-delimited frames of at most 1 MiB over a sandboxed process; a
  reader thread drains stdout and stderr continuously (stderr goes to the log
  buffer, non-JSON stdout lines are logged and ignored).
- Streamable HTTP: one POST per message with `Accept: application/json,
  text/event-stream`; JSON or SSE responses (1 MiB per message), server
  requests and notifications interleaved before the response, `Mcp-Session-Id`
  and `MCP-Protocol-Version` once known, DELETE at close. Redirects are never
  followed, so credential headers cannot be carried elsewhere. HTTP 401/403 is
  `needs_auth`; 404 with a session is `session_expired`.
- `initialize` (versions `2025-06-18`, `2025-03-26`, `2024-11-05`; `clientInfo`
  `AiDEX`), `notifications/initialized`, `tools/list` following `nextCursor` (at
  most 64 pages and 512 tools; schemas over 64 KiB replaced), `tools/call`
  (results reduced to at most 256 KiB of redacted text; images, audio, and blobs
  are summarised), and `ping`. Server `ping` requests are answered; sampling,
  roots, and elicitation requests are declined.
- Every request has a timeout (`Timeouts`: initialize 60 s, list/ping 30 s, call
  300 s) and honours a `CancelToken`; an abandoned request gets
  `notifications/cancelled`, and its late answer is ignored.

Everything a server sends is scrubbed of the secret values resolved for it
(including the token inside `Bearer <token>`) before it is parsed, logged, or
returned. `LogBuffer` keeps the newest 400 lines / 128 KiB per server (stderr,
`notifications/message`, and lifecycle notes), each redacted and cut to 2 KiB.

## Supervisor

`McpSupervisor` holds one slot per server: status `stopped | starting | running
| failed | needs_auth`, the last error (`{reason, message}`), the tool list, the
server identity, and the log buffer.

- `start`, `stop`, `restart`, `stop_all(reason)`, `forget(id)`.
- `list_tools(id, cancel)` and `call_tool(id, tool, arguments, cancel)` start an
  enabled server on demand; a call to a tool the server did not list is refused.
  A `tools/list_changed` notification refreshes the list before the next request.
- `running_toolsets(workspace)` reports the tools of enabled, running servers
  offered in a workspace (for the harness), with `auto_approve_tools`.
- A connection serves one request at a time. `stop` interrupts an in-flight
  request or start (`cancelled`), then closes: stdio servers get end-of-input and
  two seconds before the tree is killed; HTTP sessions are deleted.

`McpHost` is how the supervisor reaches servers. `SandboxHost` is the production
host: stdio servers run through `aidex_sandbox::Sandbox::spawn_interactive` at
`brokered` level with a cleared environment (`DEFAULT_ENV_ALLOWLIST` plus the
server's `env_allowlist`), a per-server scratch home/temp directory, and the
secrets of `env_from_secrets`, resolved through a `SecretSource` (`VaultSecrets`:
a five-second vault handle scoped to `mcp:<id>`, revoked right after use).
`McpGate` checks every launch and every remote request; `OpenGate` allows
everything (tests and embedders).

## Daemon composition

`aidexd` (`services/mcp.rs`) launches through the broker's sandbox, so stdio
servers live in the broker's process registry and die with emergency stop,
daemon shutdown, and drive removal. Its gate refuses launches during an
emergency stop and, before every request to a remote server, applies the offline
switch (loopback servers stay usable), the emergency stop, and a
`network.request` intent for the classified host under the drive's policy
settings and documents (approval verdicts refuse, since a request cannot wait;
each decision is a `policy.decision` event). A watchdog stops every server on
daemon shutdown, vault lock, and emergency stop. Launches record
`sandbox.negotiated`, `process.started`, and `process.exited`.

## Limits

- Servers run at sandbox level 1 (`brokered`): cleared environment, a killable
  process tree, and no filesystem or network confinement; the server's own
  network use is not mediated.
- The server-initiated HTTP GET stream and resumable SSE are not used; legacy
  SSE-only servers are imported as Streamable HTTP and must accept POST.
- OAuth for remote servers is not implemented: headers come from vault secrets.
- Resources, prompts, sampling, roots, and elicitation are not supported.

## Tests

`cargo test -p aidex-mcp` runs the unit tests plus `tests/stdio.rs` (the Python
fixture `tests/fixtures/mcp_server.py` through the real sandbox helper:
initialize, paginated tools/list, tools/call, tool errors, server requests,
timeouts, crashes, cleared environment, secret scrubbing, supervisor lifecycle),
`tests/http.rs` (a loopback fake Streamable HTTP server), and `tests/store.rs`.
Build the helper first (`cargo build --workspace --bins`); the interpreter is
`AIDEX_TEST_PYTHON` (set by CI), falling back to a conventional location.
