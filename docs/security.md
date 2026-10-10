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

## Workspace confinement

`aidex-workspace` is the first enforced filesystem boundary. Workspace roots are only directories the user explicitly selected (never the drive, a directory containing it, or AiDEX-managed directories) or directories AiDEX created on the drive. `ConfinedRoot::resolve` rejects traversal, absolute, drive-letter, and UNC paths, alternate data streams, and Windows reserved or aliasing names; it follows a symbolic link or junction only when the resolved target stays inside the root and fails closed on dangling or looping links. Ephemeral copies are applied only after review, per selected file, with conflict detection and a review digest; deletion is limited to marker-verified, AiDEX-created directories, and host data is never deleted. Resolution is a point-in-time check rather than OS-enforced isolation: a process that can modify the workspace concurrently can race it (TOCTOU) until the sandbox confines tool processes.
