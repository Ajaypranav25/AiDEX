//! Portable local GGUF inference (spec §10, §11, §46, §66, §67, §86, §96, §97).

mod error;
mod fsutil;
pub mod gguf;
pub mod runtime;

pub use error::{InferenceError, Result};
