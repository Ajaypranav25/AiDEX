# Approval service

Implemented by the execution broker: see [`crates/broker`](../broker/README.md) (`aidex_broker::approvals`). Approvals are human-readable cards bound to the exact intent fingerprint, workspace, session, and expiry, stored durably with atomic state transitions, resolvable once or for the session, revocable, and audited as `approval.*` events. The daemon serves them as `approval.list|show|resolve|revoke` and the CLI as `aidex approvals`, `aidex approve`, and `aidex deny`. Remote approvals (scoped, authenticated commands from paired devices) are issue #29.
