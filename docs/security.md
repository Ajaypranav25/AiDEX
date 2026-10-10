# Security design constraints

AiDEX mediates agent actions rather than trusting model output. The policy and execution broker are the intended boundary for shell, process, filesystem, network, package, secret, and remote operations.

## Required properties

- Treat repository instructions, tool output, MCP results, and web content as untrusted data.
- Default to explicit scoped access; never expose the whole host filesystem or vault to an agent.
- Store durable private data encrypted and keep credentials out of logs, model context, and source-controlled manifests.
- Require authentication, replay protection, and permission scopes for remote control.
- Verify signed updates and runtime integrity before execution.
- Describe cleanup limits honestly: AiDEX controls only residue it created, not privileged host telemetry.

These properties are requirements from the product specification, not current implementation claims. No vault, broker, or remote security boundary is production-ready in this foundation; the process sandbox below enforces exactly what its plans state and nothing more.

## Process sandbox

`aidex-sandbox` contains processes AiDEX launches. Callers request a level (0 direct, 1 brokered, 2 restricted, 3 ephemeral), mounts, network mode, and limits; negotiation returns an `EnforcementPlan` naming the mechanism for every requirement, or fails with the list of requirements this host cannot enforce. Nothing is silently downgraded, and level 0 requires an explicit `allow_direct` opt-in.

- Every child starts from a cleared environment plus a credential-free allowlist; credential-like names cannot be inherited, and secrets reach a tool only as explicit per-launch values that never appear in plans, logs, or `Debug` output.
- Every tree is killable as a unit (Windows Job Object, Unix process group) and tracked by a registry that cancel, daemon shutdown, emergency stop, and drive removal terminate with a per-process report.
- Windows enforces level 1 only: there is no filesystem or network confinement without unsafe Win32 calls, and process/memory/CPU limits are watchdog-enforced (sampled, tree killed on violation), not kernel-enforced. Level 1 does not contain hostile code running as the same user.
- Linux enforces level 2 with Landlock (filesystem), rlimits, and user/network namespaces when the kernel allows them; otherwise those requirements fail closed. A descendant can leave the process group with `setsid()`.
- macOS uses `sandbox-exec` profiles for write confinement and network denial; reads are not confined and the backend is validated only in CI.

See `crates/sandbox/README.md` for the full enforcement matrix and limitations.