# Daemon API server

The v1 transport binds only to IPv4 loopback and accepts one newline-delimited
JSON request per connection. Every request carries contract version 1, a 256-bit
OS-random bearer capability, and exactly one of `handshake`, `status`, `stop`, or
`events`. Unknown fields, malformed JSON, unauthorized capabilities, and
incompatible versions are rejected. The endpoint file is rooted at
`<drive>/.aidex/runtime/daemon.json`; Unix permissions are owner-only (0600).
On Windows the endpoint capability is protected with the current user's DPAPI
credential and cannot be recovered by another Windows account.

The event database lives at `<drive>/.aidex/runtime/events.sqlite`. Startup
refuses symlinks at the managed `.aidex`, runtime, and event database paths. The
`events` request requires a workspace ID, an acknowledged `after` sequence, and
a batch limit from 1 to 256. The daemon returns persisted events strictly after
that cursor, or an explicit retention-expired error. It commits transitions
before responding; repeated requests are safe after disconnect because callers
advance only through events they have consumed. The CLI's `--follow` mode polls
from its latest flushed cursor and reconnects after daemon restarts, avoiding a
read-to-subscribe race without a server-side unbounded queue.

The protocol grants no filesystem or command-execution capability. Event fields
are fixed typed identifiers and transition outcomes; secrets, credentials,
prompts, and raw tool/model output are not event payloads.