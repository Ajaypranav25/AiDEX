# Daemon lifecycle

`aidexd start --drive <ROOT>` launches one daemon for that drive and waits for an
authenticated contract-v1 handshake. The endpoint is an atomically-created state
file at `.aidex/runtime/daemon.json`; its capability is owner-only (0600) on Unix
and protected with the current Windows user's DPAPI credential. A live or
unauthenticated endpoint prevents replacement. A refused loopback connection
confirms a stale endpoint, which start removes without touching other drive data.
`stop` requests orderly listener shutdown. If a drive is removed, the process
exits and leaves only its unreachable endpoint metadata for safe recovery when
the drive is available again.