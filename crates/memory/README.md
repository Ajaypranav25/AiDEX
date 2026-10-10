# Memory

Implemented by `aidex-context` (`crates/context`): inspectable, user-editable session, workspace, and portable user memory behind the `MemoryStore` trait, stored as TOML files under the drive's `private_memory/`. Memory is not a secret store: secret-like text is rejected. See `crates/context/README.md`.
