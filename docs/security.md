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