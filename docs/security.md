# Security design constraints

AiDEX mediates agent actions rather than trusting model output. The policy and execution broker are the intended boundary for shell, process, filesystem, network, package, secret, and remote operations.

## Required properties

- Treat repository instructions, tool output, MCP results, and web content as untrusted data.
- Default to explicit scoped access; never expose the whole host filesystem or vault to an agent.
- Store durable private data encrypted and keep credentials out of logs, model context, and source-controlled manifests.
- Require authentication, replay protection, and permission scopes for remote control.
- Verify signed updates and runtime integrity before execution.
- Describe cleanup limits honestly: AiDEX controls only residue it created, not privileged host telemetry.

These properties are requirements from the product specification, not current implementation claims. No vault, sandbox, broker, or remote security boundary is production-ready in this foundation.

## Policy evaluation (implemented) and enforcement (not yet)

`crates/policy` (`aidex-policy`) implements deterministic policy evaluation: agents' requests are normalized into intents carrying broker-computed facts, and evaluation returns allow, deny, sandbox, rate-limit, or approval with the deciding rule and a human-readable effect summary. Hard safety rules (disk formatting, raw drive writes, credential export, secret dumping, deleting root/home/system directories, vault modification, broker disabling, AiDEX-protected paths) deny unconditionally, including when a shell command implies them; policy changes and destructive commands always need one-time approval. Any layer deny wins; otherwise the most specific layer, then the active profile, then deny-by-default decides. Privacy mode, network mode, host trust, and repository project rules can only tighten a decision. Policy files are closed-schema TOML and fail closed when malformed. Intent fingerprints let approvals bind to the exact intent.

This is a decision engine only. No execution broker routes or enforces agent actions yet, approvals are not persisted or exposed through the CLI, and decisions are not yet audit events; those land with the broker work for issue #17. The shell classifier is a conservative heuristic, and network destinations are classified before DNS resolution, so enforcement must still re-check resolved addresses and rely on the sandbox.

## Workspace confinement

`aidex-workspace` is the first enforced filesystem boundary. Workspace roots are only directories the user explicitly selected (never the drive, a directory containing it, or AiDEX-managed directories) or directories AiDEX created on the drive. `ConfinedRoot::resolve` rejects traversal, absolute, drive-letter, and UNC paths, alternate data streams, and Windows reserved or aliasing names; it follows a symbolic link or junction only when the resolved target stays inside the root and fails closed on dangling or looping links. Ephemeral copies are applied only after review, per selected file, with conflict detection and a review digest; deletion is limited to marker-verified, AiDEX-created directories, and host data is never deleted. Resolution is a point-in-time check rather than OS-enforced isolation: a process that can modify the workspace concurrently can race it (TOCTOU) until the sandbox confines tool processes.
