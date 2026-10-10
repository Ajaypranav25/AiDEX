# `aidex-sessions`

Durable agent sessions on the drive (spec §4.6, §20-22, §39, §62, §74, §77, §81, §87):
session records with an explicit state machine, messages, tool calls, approval
references, typed artifacts, hash-verified logical checkpoints, branching, startup
recovery after a crash, and resume plans for continuing on the same or another host.
It replaces the former process-local `SessionCatalog`. Continuing an agent loop is
the harness's job (#19); this crate provides the durable state and the plan.

Depends on `aidex-core`, `aidex-drive`, `aidex-events` (`EventSink`, redaction),
`aidex-storage` (`FileStore` blobs), SQLite, and `gethostname`.

## Layout on the drive

| Path | Contents |
|---|---|
| `<private_sessions>/sessions.sqlite3` | sessions, messages, tool calls, approval refs, artifact and checkpoint records, event gaps, the event outbox (WAL, `synchronous=FULL`, `PRAGMA user_version` = 1) |
| `<private_sessions>/store/` | `aidex-storage` `FileStore`: SHA-256-addressed checkpoint envelopes and artifacts, verified on every read |
| `<private_config>/host-salt` | 32 random bytes, created once per drive, salting host fingerprints |

Records hold ids, timestamps, and drive-relative data only — never absolute host
paths — so session ids, history, and checkpoints survive a daemon restart and a
drive move (another drive letter or mount point).

## API

```rust
use aidex_sessions::*;

let store = SessionStore::open(&paths, Arc::new(event_log.clone()))?;   // or open_with_host(paths, sink, host)
store.host() -> &HostFingerprint;

// Sessions
store.create(NewSession::new(workspace_id, agent_id, "title").with_parent(p).with_model(m))? -> Session;
store.get(&id)? / store.list(&SessionFilter::for_workspace(ws).with_state(SessionStateKind::Running).with_limit(50))?;
store.transition(&id, SessionState::Running)? -> Session;        // InvalidTransition if the table forbids it
store.interrupt(&id, "drive removed")?;                            // recoverable = a checkpoint verifies
store.set_model(&id, Some(model))?; store.add_budget_usage(&id, usage)?; store.set_event_cursor(&id, cursor)?;
store.summary(&id)? -> SessionSummary { session, messages, tool_calls, open_tool_calls, artifacts, checkpoints, approvals, latest_checkpoint, event_gaps };

// History
store.append_message(&id, NewMessage::new(MessageRole::User, text).for_tool_call(call))? -> Message { seq, .. };
store.messages(&id, after_seq, limit)? -> MessagePage { messages, next_after, has_more };
store.record_tool_call(&id, NewToolCall::new(call_id, tool_id, input_json))? -> ToolCallRecord;
store.update_tool_call(&id, &call_id, ToolCallUpdate::state(ToolCallState::Completed).with_summary(s).with_artifact(a))?;
store.tool_call(&id, &call_id)? / store.tool_calls(&id)?;
store.record_approval_ref(&id, ApprovalRef { approval_id, intent_fingerprint_hex, decision, scope, expires_at })? -> ApprovalRecord;
store.approvals(&id)?;
store.put_artifact(&id, NewArtifact::new(ArtifactKind::Patch, producer, bytes).with_sources(paths))? -> ArtifactRef;
store.artifact(&artifact_id)? / store.read_artifact(&artifact_id)? / store.artifacts(&id)?;

// Checkpoints and branches
store.checkpoint(&id, Some("label"), LogicalState::new(harness_json).with_task_graph(graph))? -> CheckpointMeta;
store.checkpoints(&id)? -> Vec<CheckpointMeta>;
store.latest_valid_checkpoint(&id)? -> Option<RestoredCheckpoint { checkpoint, recovered_from_corruption, corrupt_checkpoints }>;
store.load_checkpoint(&id, &checkpoint_id)? -> Checkpoint { meta, host_fingerprint, seeded_from, marks, state };
store.branch(&from, &checkpoint_id)? -> Session;

// Recovery and resume
store.recover_on_startup(&current_host, Some(event_log.latest_cursor()?))? -> RecoveryReport;
store.event_gaps(&id)? -> Vec<EventGap>;
store.resume_plan(&id, &current_host)? -> ResumePlan;
store.mark_resumed(&id)? -> Session;                               // interrupted → paused, adopted on this host

// Host identity and secrets
HostFingerprint::for_drive(&paths)? / HostFingerprint::compute(salt, os, arch, hostname) / fp.changes_from(&previous);
contains_secret_like(text) -> bool;
```

Errors are `SessionError` with `code() -> ServiceErrorCode` and a stable `reason()`
(`session_not_found`, `checkpoint_not_found`, `artifact_not_found`,
`tool_call_not_found`, `invalid_transition`, `session_closed`, `invalid_argument`,
`conflict`, `checkpoint_corrupt`, `no_valid_checkpoint`, `incompatible`, `corrupt`,
`unsafe_path`, `unavailable`, `storage`).

## State machine and events

| From | Allowed to |
|---|---|
| `created` | `running`, `failed`, `cancelled` |
| `running` | `waiting_approval`, `paused`, `completed`, `failed`, `cancelled`, `interrupted` |
| `waiting_approval` | `running`, `paused`, `failed`, `cancelled`, `interrupted` |
| `paused` | `running`, `failed`, `cancelled` |
| `interrupted` | `running`, `paused` (adopted by `mark_resumed`), `failed`, `cancelled` |
| `completed`, `failed`, `cancelled` | — (terminal; no new messages, tool calls, artifacts, or checkpoints) |

Events, in the session's scope (`EventScope::session(workspace, session)`):
`session.created` (create, branch), `session.started` (created → running),
`session.resumed` (paused/interrupted → running), `session.paused`,
`session.completed`, `session.failed` (`"<code>: <message>"`), `session.cancelled`,
`session.interrupted`, `session.checkpointed`. Entering and leaving
`waiting_approval` emit nothing here: the broker's `approval.*` events report it.
The store does not emit `agent.*`, `tool.*`, or `model.*` events; their owners do.

Each event is staged in an outbox row in the same transaction as the change, emitted
after commit, and then cleared while the session's `last_event_cursor` advances. A
failed append keeps the committed change and records an `emit_failed` gap; a crash
between commit and append leaves the outbox row, which startup turns into an
`interrupted` gap. Events are never re-emitted or synthesised afterwards.

## Checkpoints

A checkpoint is a JSON envelope (format 1): checkpoint/session/workspace ids, time,
label, the host it was written on, the harness-owned `LogicalState { harness,
task_graph }`, and store-captured `HighWaterMarks { message_seq, tool_calls,
event_cursor, model, open_approvals, budget, session_state }`. It is stored in the
`FileStore`; SHA-256, size, and format are recorded in SQLite. Restoring verifies
hash, size, format, and that the envelope names its own record. A failing checkpoint
is marked `corrupt` (with a reason, never deleted) and the next older valid one is
returned with `recovered_from_corruption`; when every checkpoint fails the result is
`NoValidCheckpoint`. A newer envelope format is `Incompatible`, not corruption.

A branch is a new `created` session in the same workspace with `parent_session_id`
and `branched_from` set, a copy of the parent's messages up to the checkpoint, and a
seed checkpoint holding the same logical state. Approvals, budget, tool calls, and
the event cursor are not inherited — an approval granted to the parent never
applies to a branch.

## Recovery, resume, and hosts

`recover_on_startup` runs once at daemon startup: outbox rows become gaps; when the
event log's latest cursor is known, a session cursor ahead of it becomes a
`log_behind` gap and is lowered (the missing events are not recreated); every
`running` tool call becomes `lost`; every `running`/`waiting_approval` session has
its pending approvals cancelled and becomes `interrupted { reason: "daemon
restarted", recoverable }`. It is idempotent.

`resume_plan` returns the session, the newest verified checkpoint, messages after it,
event gaps, `different_host` and readable `host_changes`, and `unrecoverable` items:
lost processes (`process`), lost or never-run tool calls (`tool_call`), approvals that
must be asked again (`approval`), and in-memory model state (`model_state`; plus the
selected model when the host changed). `mark_resumed` adopts an interrupted (or
paused) session on this host and leaves it `paused`; the harness continues it by
moving it to `running`.

`HostFingerprint { id, os, arch }`: `id` is 128 bits of SHA-256 over a domain tag,
the drive's random salt, OS, architecture, and the lower-cased hostname. The same
machine is unlinkable across drives, the hostname is never stored, and no hardware
identifiers are read. A malformed salt file is reported as corrupt, never replaced.

## Secrets

Session records must hold vault handles or `SecretRef`s, never secret values. As
defence in depth every persisted text field — titles, messages, tool inputs (JSON,
redacted with their keys) and summaries, failure and interruption reasons, checkpoint
labels and payloads, and UTF-8 artifacts — passes through `aidex_events::redact_text`
before it is written; `redacted` flags on messages, tool calls, and artifacts report
replacements.

## Limitations

- Redaction is pattern based and cannot recognise arbitrary secrets; binary
  artifacts are stored unchanged.
- The database and blobs are not encrypted at rest (drive encryption is a later
  milestone).
- The store records state; it does not run or stop agents. Cancelling a session only
  changes its state until the harness (#19) wires cancellation to running work.
- Checkpoint retention/pruning and artifact garbage collection are not implemented.
- A gap is reported for an outbox row even if the event did reach the log just
  before a crash (over-reporting rather than inventing history).
