# `aidexd`

The local AiDEX daemon: it composes backend services for one drive and exposes
them as JSON-RPC 2.0 methods over an authenticated loopback connection (IPC
contract version 2). The crate also ships the blocking client the `aidex` CLI
uses. Wire format, handshake, error codes, and naming rules:
[`protocol/commands/README.md`](../../protocol/commands/README.md).

## Layout

| Module                | Responsibility                                                                                                                                                                                                                                                                 |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `endpoint`            | `daemon.json` endpoint file, `Capability` (redacted `Debug`, constant-time `matches`), DPAPI/0600 protection, symlink-checked `.aidex/runtime`                                                                                                                                 |
| `rpc`, `errors`       | JSON-RPC message model, params helpers, the stable `ErrorCode` table, `RpcError`                                                                                                                                                                                               |
| `handshake`           | `aidex.handshake` params/result and server-side validation                                                                                                                                                                                                                     |
| `router`              | `Router`, `CallContext`, `ClientInfo`                                                                                                                                                                                                                                          |
| `stream`              | `NotificationSink` for subscription methods                                                                                                                                                                                                                                    |
| `transport` (private) | listener, per-connection reader/writer threads, bounded outbox, shutdown                                                                                                                                                                                                       |
| `services`            | `Services`, built once and shared with handlers as `Arc<Services>`                                                                                                                                                                                                             |
| `methods/<domain>`    | JSON-RPC adapters: `daemon.*` (status/ping/stop), `events.*` (replay/latest/subscribe), `vault.*` (status/init/unlock/lock/secret.set/secret.list/secret.delete/passphrase.change), `workspace.*` (registry, discovery, ephemeral review/apply/discard over `aidex-workspace`) |
| `lifecycle`           | `Daemon`, `DaemonHandle`, `serve`, `launch`, `stop`, stale-endpoint recovery                                                                                                                                                                                                   |
| `client`              | `Client`, `Subscription`, `ClientError`                                                                                                                                                                                                                                        |

## API

```rust
// Server side (inside aidexd)
router.register("domain.verb", |ctx: &CallContext, params: Value| -> Result<Value, RpcError> { .. })?;
router.register_stream("domain.subscribe", |ctx, params, sink: NotificationSink| -> Result<Value, RpcError> {
    std::thread::spawn(move || while sink.notify("domain.event", json!({..})).is_ok() { .. });
    Ok(json!({ "cursor": 0 }))          // becomes the subscribe result's `data`
})?;

// Client side (CLI, tests, other frontends)
let mut client = Client::connect(drive_root, "aidex-cli")?;
let status: DaemonStatus = client.call_as("daemon.status", Value::Null)?;
let mut events = client.subscribe("domain.subscribe", json!({}))?;
while let Some(notification) = events.next_timeout(Duration::from_secs(1))? { .. }
```

`ClientError` is `Unavailable | Unauthorized | Incompatible { message, supported } |
Rpc { code, message, data } | Io | Protocol`; `rpc_kind()` maps `Rpc` codes back
to `ErrorCode`. Every error type has a stable `code() -> aidex_core::ServiceErrorCode`.

Embedding and tests: `Daemon::with_options(root, DaemonOptions { transport, poll_interval, drive_present, updater })`
(`updater: Option<aidex_updater::UpdaterConfig>` injects a trust store and self-test; `None` uses the built-in trust store),
`daemon.router_mut()` for extra methods, `daemon.start()` → `DaemonHandle`
(`endpoint()`, `stop()`, `wait()`; dropping it stops the daemon), and
`Client::connect_endpoint(handle.endpoint(), ClientInfo::new(..))` to skip the
endpoint file. `Router::call` and `CallContext::detached` unit-test plain
handlers without sockets.

## Adding a domain service

1. **Service** — in `services.rs`, add a private field (e.g. `sessions: SessionStore`),
   open it in `Services::open` mapping failures with
   `ServicesError::new("sessions", error.code(), error.to_string())`, and add an
   accessor `pub fn sessions(&self) -> &SessionStore`. Locate drive files through
   `services.paths()` (`aidex_drive::DrivePaths`; the daemon only runs on an initialized
   schema-v2 drive) and create directories with `paths().ensure_dir(..)`.
2. **Methods** — create `methods/sessions.rs` with typed `#[serde(deny_unknown_fields)]`
   params/results, `pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError>`
   that clones the `Arc` into each handler, decodes params with `rpc::parse_params`,
   maps domain errors with `RpcError::service(error.code(), error.to_string())`
   (or a specific `ErrorCode` such as `PolicyDenied`, `VaultLocked`), and returns
   `rpc::to_result(&dto)`. Streams use `register_stream("sessions.subscribe", ..)`
   and push `sessions.event` notifications.
3. **Wire it** — add `pub mod sessions;` and `sessions::register(router, services)?;`
   to `methods/mod.rs`.
4. **CLI** — create `crates/aidex-cli/src/commands/sessions.rs` with a clap
   `Subcommand` enum and `pub fn run(command, output: Output) -> Result<Exit, CliError>`
   that calls `commands::daemon::connect(drive)?` and `client.call_as(..)`; add
   `pub mod sessions;` plus a dispatch arm in `commands/mod.rs`, a variant in
   `cli::Commands`, and its label in `Commands::label`.
5. **Tests** — unit-test handlers with `Router::call`; test wire behaviour with an
   in-process `Daemon` (see `tests/common`).

## Guarantees

- Binds IPv4 loopback only; a 256-bit OS-random capability is required in the
  first message, compared in constant time, protected at rest (DPAPI on
  Windows, 0600 on Unix), never logged, and redacted from `Debug`.
- One daemon per drive (`create_new` endpoint); stale endpoints are removed
  only when provably unreachable; other drive data is never touched.
  On Windows, clients check loopback reachability before unlocking the temporary
  DPAPI credential. Moving the drive to another computer therefore lets startup
  replace an unreachable endpoint with that computer's credential, preserving the
  portable vault and history. Reachable peers still require authentication and
  are never replaced on a credential error.
- Bounded everything: 1 MiB lines, 64 connections, 1024 queued messages /
  16 MiB per connection, 64 subscriptions per connection, 5 s handshake.
  Slow consumers are disconnected instead of buffering without limit.
- The daemon stops (closing connections and removing its endpoint) on
  `daemon.stop` or when the drive disappears.
- Updates: right after publishing its endpoint (so this daemon owns the drive)
  and before serving, the daemon runs `Updater::recover`, finishing or rolling
  back an interrupted update; `update.apply`/`update.rollback` switch
  `app/current.json` but never replace the running binaries (the new version
  starts on the next launch). `aidexd --version` prints the version (used by the
  update self-test).

## Limitations

- Methods: `daemon.status|ping|stop`, `events.replay|latest|subscribe`, `workspace.*`,
  `vault.*`, `model.*`, `provider.*`, `network.*`, and `update.status|check|apply|rollback`
  exist, plus the broker's
  `tool.list|invoke|continue`, `broker.approval.list|show|resolve|revoke`,
  `policy.show|profile.set|reload|explain`, and `emergency.stop|resume`; additional
  domains are future work.
- `Services::broker()` returns the `Arc<aidex_broker::Broker>` composed over the drive
  (approvals in `private_sessions/approvals.sqlite3`, settings in
  `private_config/policy.toml`, documents in `policies/`, sandbox scratch in
  `tmp/broker/`, the helper `aidex-sandbox-exec` next to the daemon, the drive vault).
  A background thread expires due approvals every 500 ms; `session.cancel` also
  cancels the session's calls and approvals; daemon stop and drive removal terminate
  every managed process. Every IPC client acts as the local user. Remote provider
  calls pass the broker's `ModelEgress` gate (offline switch, emergency stop,
  `model.invoke` policy); vault-backed provider credentials go through it too
  (`secret.use`, audited 30 s handles), while `aidex-auth` account sign-ins resolve
  through `AuthService` (`BrokeredCredentials`).
- The event log is `DrivePaths::sessions()/events.sqlite3` (100 000 events, 90
  days). PR #40's `.aidex/runtime/events.sqlite` is superseded: it is imported once,
  read-only, into an empty new log and then left untouched (safe to delete).
  The daemon records `daemon.started` once its endpoint is published and
  `daemon.stopping` before it closes connections (best effort; live subscribers
  may only see it on replay). Each `events.subscribe` stream uses one thread.
  The stream sends envelopes as `events.event` and consumed durable progress as
  `events.watermark {cursor}`. Watermarks follow all matching events they cover,
  including filtered history and live queue catch-up; clients acknowledge them
  after processing preceding events. They are not events or periodic heartbeats,
  and ephemeral events and the initial `latest_cursor` do not acknowledge progress.
- `Services::vault()` returns the drive's `Arc<aidex_vault::VaultService>` (vault in
  `DrivePaths::vault()`, default 30 min idle auto-lock). A damaged vault does not stop
  the daemon; `vault.status` reports it as `unavailable`. Secret handles are issued and
  resolved through `VaultService` in-process (for the broker), never over IPC.
- DTOs for daemon methods live in this crate (`methods::*`), not in the
  generated `aidex-core` contract schema yet.
- No per-client authorization beyond the bearer capability; any process of the
  same OS user that can read the endpoint file can connect.
- See `src/transport/README.md` and `src/lifecycle/README.md` for transport and
  lifecycle specifics.

## Provider/model methods

`Services::models()` exposes `ModelServices` with a shared `ProviderRegistry`,
model router, offline switch and vault credential resolver. Model chat stays an
internal `ModelProvider` API; desktop messages use the separate `conversation.*`
and `agent.*` services rather than a raw provider-chat RPC.

| Method             | Parameters                                                                                     | Result                                                                                                           |
| ------------------ | ---------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `provider.presets` | none                                                                                           | `{presets: [ProviderPreset]}`; stable IDs, protocol kind, endpoint defaults, capabilities and auth/OAuth support |
| `provider.add`     | `{id, kind?, base_url, secret_name?, locality?, discover_models?, models?}`                    | `ProviderEntry`                                                                                                  |
| `provider.update`  | same full replacement configuration as add                                                     | `ProviderEntry`; validates/persists before replacing the instance                                                |
| `provider.remove`  | `{id}`                                                                                         | `{removed}`; clears a default pointing at this account                                                           |
| `provider.health`  | `{id}`                                                                                         | `{provider_id, status, error?, message?, latency_ms?, checked_at}`                                               |
| `model.providers`  | none                                                                                           | `{providers: [ProviderEntry]}`                                                                                   |
| `model.list`       | none                                                                                           | `{models, failures, default_model, network}`                                                                     |
| `model.route`      | `{task, privacy?, required_context?, needs_tools?, needs_json_schema?, secret_heavy?, model?}` | `{decision, model, network, failures}`                                                                           |
| `model.use`        | `{provider_id, model_id}`                                                                      | `{default_model, model}`; validates then persists                                                                |
| `model.default`    | none                                                                                           | `{default_model}`                                                                                                |
| `network.offline`  | `{enabled}`                                                                                    | `{offline, network}`                                                                                             |
| `network.status`   | none                                                                                           | `{offline, network}`                                                                                             |

Provider kinds are `openai-compatible`, `openai-responses`, `anthropic`, `gemini`,
`azure-openai`, and `bedrock`. Runtime local providers also use `llama-cpp` or
`fake`, which cannot be configured through `provider.add`. `secret_name` names a
vault secret with consumer `provider:<id>`; keys are set through `vault.secret.set`.
Credential handles are short-lived, resolved and revoked per call. Health reports
`auth_required` for missing/locked/rejected credentials. API-key provider presets
retain `oauth_supported: false`; supported ChatGPT account acquisition is owned
by the shared `aidex-auth` service and exposed through `auth.*`. Its encrypted
accounts register OpenAI Responses provider instances after identity verification.

### Standalone desktop conversations

`conversation.create` with omitted or null `workspace_id` creates a project-free chat, even when `workspace.select` has a current project. Explicit references must resolve to registered, available workspace handles. `agent.configuration.set` performs the same validation before persistence and preserves message history. Plan/Agent execution requires a selected project; ordinary Chat does not expose agent tools.
