//! Dependency-free HTML handling: a forgiving tokenizer, character
//! references, and readable-text extraction.

mod entities;
mod extract;
pub(crate) mod tokenizer;

pub use entities::decode as decode_entities;
pub use extract::{Readable, extract, plain_text};
pub(crate) use extract::truncate_chars;
