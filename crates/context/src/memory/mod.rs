//! Inspectable, editable memory in three scopes (spec §35): session, workspace,
//! and portable user memory. Memory is never a secret store: secret-like text is
//! rejected with [`MemoryError::SecretLike`].

mod model;
mod store;

pub use model::{
    MAX_ENTRIES_PER_SCOPE, MAX_MEMORY_TEXT_BYTES, MAX_TAG_CHARS, MAX_TAGS, MEMORY_ID_PREFIX,
    MemoryEdit, MemoryEntry, MemoryError, MemoryScope, MemoryScopeKind, MemorySource, MemoryStore,
    NewMemory, validate_tags, validate_text,
};
pub use store::{FileMemoryStore, MEMORY_SCHEMA_VERSION};
