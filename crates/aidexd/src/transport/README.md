# Daemon IPC transport (contract v2)

`aidexd` serves JSON-RPC 2.0 on an ephemeral IPv4 loopback port. The wire
contract, handshake, error-code table, and method naming rules are specified
in [`protocol/commands/README.md`](../../../../protocol/commands/README.md);
this file describes how the transport enforces them.

## Connections

- The listener accepts only loopback peers and at most `max_connections`
  (64) concurrent connections, counting ones still handshaking. Extra
  connections receive one `server_busy` (-32013) error line and are closed.
- Each connection has a reader thread and a writer thread. The reader frames
  lines (at most 1 MiB, `\n`-terminated, blank lines ignored), runs the
  handshake, and dispatches requests to the `Router` sequentially. The writer
  drains a bounded outbox, so a slow socket never blocks the reader of any
  other connection.
- The first line must arrive within `handshake_timeout` (5 s) and be an
  `aidex.handshake` request (at most 4 KiB). Wrong capability, unsupported
  contract version, malformed params, or any other first message is answered
  once and the connection is closed. The capability comparison is
  constant-time.
- Oversized lines are answered with `message_too_large` (-32012) and the
  connection is closed. After a protocol violation the reader keeps
  discarding input for up to 1 s so the client receives the error line
  instead of a TCP reset. Malformed JSON after the handshake is a normal
  `parse_error` (-32700) and the connection stays open.
- Handlers run on the reader thread and are wrapped in `catch_unwind`; a
  panicking handler yields `internal` (-32603) without affecting other
  connections.

## Backpressure

The outbox holds at most `max_queued_messages` (1024) lines and
`max_queued_bytes` (16 MiB). Producers never block: when a response or
notification would exceed either limit, the connection is closed as a
`slow_consumer` (-32015). The final `aidex.closing` notification may exceed the
limits so it is still attempted; a socket write blocked longer than
`write_timeout` (10 s) drops the connection. Every subscription on the
connection then fails with `SinkError::Overflow`/`Closed`, so a stalled client
can neither stall the daemon nor grow its memory without bound.

## Streams

`Router::register_stream` handlers receive a `NotificationSink`. The
transport registers a per-connection subscription id (`sub-<n>`, at most
`max_subscriptions` = 64 per connection), answers the subscribe call with
`{"subscription": id, "data": <handler result>}`, and guarantees that answer
is written before any notification: until the handler returns, sends from
other threads wait and sends from the handler thread are held back (bounded).
Dropping the sink ends the stream with `aidex.unsubscribed`; `aidex.unsubscribe`
from the client, connection close, or daemon shutdown make further sends fail.

## Shutdown

On `daemon.stop`, `DaemonHandle::stop`, or drive removal the listener stops
accepting, every connection gets `aidex.closing` (`shutting_down`, -32014) and
drains its in-flight response, connections still open after
`shutdown_grace` (5 s) are force-closed, and the endpoint file is removed.

## Limits of this design

- Requests on one connection are processed in order; a slow handler delays
  later requests on the same connection (use a stream for long work).
- `CallContext::is_cancelled` reports connection closing and daemon shutdown,
  but a client disconnect is only observed after the running handler returns.
- The capability is a bearer secret: any process of the same OS user that can
  read the endpoint file can connect. There is no per-client authorization yet.
