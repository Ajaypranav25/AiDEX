# Daemon API server

The v1 transport binds only to IPv4 loopback and accepts one newline-delimited
JSON request per connection. Every request carries contract version 1, a 256-bit
OS-random bearer capability, and exactly one of `handshake`, `status`, or `stop`.
Unknown fields, malformed JSON, unauthorized capabilities, and incompatible
versions are rejected. The endpoint file is rooted at
`<drive>/.aidex/runtime/daemon.json`; Unix permissions are owner-only (0600).
On Windows the endpoint capability is protected with the current user's DPAPI
credential and cannot be recovered by another Windows account.

The protocol is deliberately narrow: it grants no filesystem or command-execution
capability. Endpoint secrets remain private to the local user; Windows credential
protection is independent of the drive's inherited access controls.