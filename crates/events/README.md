# `aidex-events`

`EventLog` is the backend event contract: version 1 typed transition records, ordered per-workspace sequence cursors, optional session scope, SQLite durability, bounded retention, cursor replay, and bounded best-effort in-process subscriptions. Events are committed before live notification. Subscriber queues and replay batches are capped at 256; a slow subscriber is detached and reconnects by reading after its last acknowledged sequence.

Event outcomes and approval decisions are closed enums; identifiers are restricted to short ASCII IDs (`A-Z`, `a-z`, digits, `.`, `_`, `:`, `-`) so free-form credentials, prompts, and output cannot be serialized as event fields. Producers must still use opaque non-secret identifiers.

Consumers advance their acknowledged cursor only after processing the corresponding event. `read_after` returns events strictly greater than the supplied cursor in sequence order. When retention has removed required history, it returns `CursorExpired` rather than silently skipping a gap. Sequence numbers are scoped to the workspace; callers must not compare cursors across workspaces.

The daemon stores the log at `<drive>/.aidex/runtime/events.sqlite` and exposes authenticated bounded replay requests over its loopback IPC protocol. `aidex events --workspace ID --after N` prints one batch as newline-delimited JSON; `--follow` keeps polling from the last successfully flushed event and reconnects after daemon outages. Polling the durable cursor avoids a race between replay and live subscription. If the cursor expires, the command exits with an explicit error; it does not advance past missing history.

Events contain only versioned, explicitly declared transition fields. Never put secrets, credentials, prompts, or raw tool/model output in event fields.
