# Security design constraints

AiDEX mediates agent actions rather than trusting model output. The policy and execution broker are the intended boundary for shell, process, filesystem, network, package, secret, and remote operations.

## Required properties

- Treat repository instructions, tool output, MCP results, and web content as untrusted data.
- Default to explicit scoped access; never expose the whole host filesystem or vault to an agent.
- Store durable private data encrypted and keep credentials out of logs, model context, and source-controlled manifests.
- Require authentication, replay protection, and permission scopes for remote control.
- Verify signed updates and runtime integrity before execution.
- Describe cleanup limits honestly: AiDEX controls only residue it created, not privileged host telemetry.

These properties are requirements from the product specification, not current implementation claims. No sandbox, broker, or remote security boundary is production-ready in this foundation.

## Vault (`crates/vault`, status: implemented, independent security review pending)

- Secrets are stored in the drive's `private/vault/` directory, encrypted with a passphrase-derived key hierarchy (Argon2id → key-encryption key → random 256-bit vault master key; HKDF-SHA256 subkeys; XChaCha20-Poly1305 records and index; HMAC-SHA256 header). It does not use any host keychain, so the drive unlocks on any supported host with the passphrase or the optional recovery key. Format, validation order, crash safety, and threat model: `crates/vault/FORMAT.md`.
- Secret names, kinds, consumer policies, and values are encrypted at rest. Tampering, truncation, swapped or foreign records, missing headers, unknown versions, and symlinked paths are explicit errors; nothing falls back to plaintext or to an empty vault. Interrupted writes leave the previous committed state readable.
- Values leave the vault only when the broker resolves a scoped, expiring, revocable handle for a consumer the secret's policy names (`provider:`, `mcp:`, `process:`, `git:`). Model prompts, agents, sessions, logs, and shell environments can never be consumers. Handle issuance and resolution are in-process Rust APIs (`VaultService`), not IPC methods. Lock, idle auto-lock (30 min default), and daemon restart invalidate every handle.
- There is no online lockout for an offline drive: passphrase strength is the protection against offline guessing. A lost passphrase without a recovery key is unrecoverable; encrypted backups protect against drive loss, not credential loss.
- Not yet done: independent cryptographic review (required before release), memory locking, zeroization of IPC request buffers, whole-vault rollback detection, audit events, phone factor, and host-trust policies.