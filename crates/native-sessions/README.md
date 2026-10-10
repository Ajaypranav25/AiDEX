# Durable encrypted native records

`SessionStore::open(path, Arc<VaultService>)` creates a SQLite native record database under the drive's sessions directory. Message bodies, configuration, complete provider messages/tool metadata, pending arguments, checkpoints and workspace memory use XChaCha20-Poly1305 with random nonces and record identity/version AAD. Only opaque IDs and lifecycle state are plaintext SQL metadata.

A random 256-bit record key is generated once while the existing vault is unlocked, stored there under `native-record-encryption-key`, and resolved through short-lived `process:native-record-store` handles. Handles are revoked per operation. No plaintext config key or long-lived decrypted key cache is used. Locked/missing/damaged vaults make private records unavailable; list/status expose only IDs/status. Authenticated decryption failures never produce an empty or plaintext history.

SQLite transactions commit each encrypted record and its run metadata together. Message command IDs bind to input hashes and return the original run when repeated; changed-input reuse fails. Startup interrupts active runs, and private state reconciles with that metadata after unlock. Native execution owns continuation and never repeats pending effects without fresh user action/planning.

The old process-local `SessionCatalog` remains exported for compatibility. It is separate from the durable native service. Independent cryptographic review and whole-database rollback detection remain release-security concerns, as with the existing vault.
