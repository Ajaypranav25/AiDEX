//! Managed, drive-resident GGUF models and loopback llama.cpp inference.
mod catalog;
mod error;
mod files;
mod gguf;
mod model;
mod provider;
mod readiness;
mod runtime;
mod service;
pub use error::InferenceError;
pub use model::*;
pub use service::InferenceService;
pub use service::LOCAL_PROVIDER_ID;
