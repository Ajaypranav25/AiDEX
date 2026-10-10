# Security design constraints

AiDEX mediates agent actions rather than trusting model output. The policy and execution broker are the intended boundary for shell, process, filesystem, network, package, secret, and remote operations.

## Required properties

- Treat repository instructions, tool output, MCP results, and web content as untrusted data.
- Default to explicit scoped access; never expose the whole host filesystem or vault to an agent.
- Store durable private data encrypted and keep credentials out of logs, model context, and source-controlled manifests.
- Require authentication, replay protection, and permission scopes for remote control.
- Verify signed updates and runtime integrity before execution.
- Describe cleanup limits honestly: AiDEX controls only residue it created, not privileged host telemetry.

These properties are requirements from the product specification, not current implementation claims. No broker or remote security boundary is production-ready in this foundation; the vault below is pending independent security review, and the process sandbox below enforces exactly what its plans state and nothing more.

## Vault (`crates/vault`, status: implemented, independent security review pending)

- Secrets are stored in the drive's `private/vault/` directory, encrypted with a passphrase-derived key hierarchy (Argon2id → key-encryption key → random 256-bit vault master key; HKDF-SHA256 subkeys; XChaCha20-Poly1305 records and index; HMAC-SHA256 header). It does not use any host keychain, so the drive unlocks on any supported host with the passphrase or the optional recovery key. Format, validation order, crash safety, and threat model: `crates/vault/FORMAT.md`.
- Secret names, kinds, consumer policies, and values are encrypted at rest. Tampering, truncation, swapped or foreign records, missing headers, unknown versions, and symlinked paths are explicit errors; nothing falls back to plaintext or to an empty vault. Interrupted writes leave the previous committed state readable.
- Values leave the vault only when the broker resolves a scoped, expiring, revocable handle for a consumer the secret's policy names (`provider:`, `mcp:`, `process:`, `git:`). Model prompts, agents, sessions, logs, and shell environments can never be consumers. Handle issuance and resolution are in-process Rust APIs (`VaultService`), not IPC methods. Lock, idle auto-lock (30 min default), and daemon restart invalidate every handle.
- There is no online lockout for an offline drive: passphrase strength is the protection against offline guessing. A lost passphrase without a recovery key is unrecoverable; encrypted backups protect against drive loss, not credential loss.
- Not yet done: independent cryptographic review (required before release), memory locking, zeroization of IPC request buffers, whole-vault rollback detection, audit events, phone factor, and host-trust policies.

## Process sandbox

`aidex-sandbox` contains processes AiDEX launches. Callers request a level (0 direct, 1 brokered, 2 restricted, 3 ephemeral), mounts, network mode, and limits; negotiation returns an `EnforcementPlan` naming the mechanism for every requirement, or fails with the list of requirements this host cannot enforce. Nothing is silently downgraded, and level 0 requires an explicit `allow_direct` opt-in.

- Every child starts from a cleared environment plus a credential-free allowlist; credential-like names cannot be inherited, and secrets reach a tool only as explicit per-launch values that never appear in plans, logs, or `Debug` output.
- Every tree is killable as a unit (Windows Job Object, Unix process group) and tracked by a registry that cancel, daemon shutdown, emergency stop, and drive removal terminate with a per-process report.
- Windows enforces level 1 only: there is no filesystem or network confinement without unsafe Win32 calls, and process/memory/CPU limits are watchdog-enforced (sampled, tree killed on violation), not kernel-enforced. Level 1 does not contain hostile code running as the same user.
- Linux enforces level 2 with Landlock (filesystem), rlimits, and user/network namespaces when the kernel allows them; otherwise those requirements fail closed. A descendant can leave the process group with `setsid()`.
- macOS uses `sandbox-exec` profiles for write confinement and network denial; reads are not confined and the backend is validated only in CI.

See `crates/sandbox/README.md` for the full enforcement matrix and limitations.

## Bundle integrity (implemented) and signatures (not yet)

The drive-root launcher verifies before it executes: the drive manifest (closed schema), the `app/current.json` pointer (which cannot leave `app/versions/` and pins the SHA-256 of the selected `bundle.json`), the manifest itself (closed schema, host target, version, drive schema, no host extraction), and the size and SHA-256 of every critical binary, refusing symbolic links, junctions, and unlisted files beside the binaries. Any mismatch stops the launch with exit code 11 and nothing runs. Installation verifies everything before committing, switches the pointer atomically, and never overwrites a drive-root file that is not an AiDEX launcher.

This is integrity, not authenticity. Nothing is signed: someone who can write to the drive can replace the launcher or rewrite the pointer and manifests, and a concurrent writer could swap a file between its check and its execution. Windows also loads the launcher's DLL imports from the drive root before any check runs. Signed manifests with a trust anchor outside the drive, SBOMs, and downgrade protection are issue #27.

## Policy evaluation (implemented) and enforcement (not yet)

`crates/policy` (`aidex-policy`) implements deterministic policy evaluation: agents' requests are normalized into intents carrying broker-computed facts, and evaluation returns allow, deny, sandbox, rate-limit, or approval with the deciding rule and a human-readable effect summary. Hard safety rules (disk formatting, raw drive writes, credential export, secret dumping, deleting root/home/system directories, vault modification, broker disabling, AiDEX-protected paths) deny unconditionally, including when a shell command implies them; policy changes and destructive commands always need one-time approval. Any layer deny wins; otherwise the most specific layer, then the active profile, then deny-by-default decides. Privacy mode, network mode, host trust, and repository project rules can only tighten a decision. Policy files are closed-schema TOML and fail closed when malformed. Intent fingerprints let approvals bind to the exact intent.

This is a decision engine only. No execution broker routes or enforces agent actions yet, approvals are not persisted or exposed through the CLI, and decisions are not yet audit events; those land with the broker work for issue #17. The shell classifier is a conservative heuristic, and network destinations are classified before DNS resolution, so enforcement must still re-check resolved addresses and rely on the sandbox.

## Event redaction (implemented)

- Events (`aidex-events`) reference secrets by handle and name only. Every free-text event field is passed through `redact_text` (provider tokens, JWTs, bearer/authorization credentials, PEM private keys, URL passwords, password/token/secret/key assignments) and bounded before it is stored or streamed, and an identifier that looks like a credential refuses the event. Redaction is pattern based and is a last line of defence, not a substitute for keeping secret values out of events. The event log is not yet encrypted at rest.

## Workspace confinement

`aidex-workspace` confines workspace-relative paths in-process (for the tools and apply logic that use it). Workspace roots are only directories the user explicitly selected (never the drive, a directory containing it, or AiDEX-managed directories) or directories AiDEX created on the drive. `ConfinedRoot::resolve` rejects traversal, absolute, drive-letter, and UNC paths, alternate data streams, and Windows reserved or aliasing names; it follows a symbolic link or junction only when the resolved target stays inside the root and fails closed on dangling or looping links. Ephemeral copies are applied only after review, per selected file, with conflict detection and a review digest; deletion is limited to marker-verified, AiDEX-created directories, and host data is never deleted. Resolution is a point-in-time check rather than OS-enforced isolation: a process that can modify the workspace concurrently can race it (TOCTOU), and containing untrusted processes is `aidex-sandbox`'s job (not yet wired to workspaces).
