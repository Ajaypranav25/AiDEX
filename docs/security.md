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

## Policy evaluation (implemented) and enforcement (not yet)

`crates/policy` (`aidex-policy`) implements deterministic policy evaluation: agents' requests are normalized into intents carrying broker-computed facts, and evaluation returns allow, deny, sandbox, rate-limit, or approval with the deciding rule and a human-readable effect summary. Hard safety rules (disk formatting, raw drive writes, credential export, secret dumping, deleting root/home/system directories, vault modification, broker disabling, AiDEX-protected paths) deny unconditionally, including when a shell command implies them; policy changes and destructive commands always need one-time approval. Any layer deny wins; otherwise the most specific layer, then the active profile, then deny-by-default decides. Privacy mode, network mode, host trust, and repository project rules can only tighten a decision. Policy files are closed-schema TOML and fail closed when malformed. Intent fingerprints let approvals bind to the exact intent.

This is a decision engine only. No execution broker routes or enforces agent actions yet, approvals are not persisted or exposed through the CLI, and decisions are not yet audit events; those land with the broker work for issue #17. The shell classifier is a conservative heuristic, and network destinations are classified before DNS resolution, so enforcement must still re-check resolved addresses and rely on the sandbox.

## Event redaction (implemented)

- Events (`aidex-events`) reference secrets by handle and name only. Every free-text event field is passed through `redact_text` (provider tokens, JWTs, bearer/authorization credentials, PEM private keys, URL passwords, password/token/secret/key assignments) and bounded before it is stored or streamed, and an identifier that looks like a credential refuses the event. Redaction is pattern based and is a last line of defence, not a substitute for keeping secret values out of events. The event log is not yet encrypted at rest.
