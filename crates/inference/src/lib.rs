//! Portable local GGUF inference (spec §10, §11, §46, §66, §67, §86, §96, §97).

pub mod composer;
mod error;
mod fsutil;
pub mod gguf;
mod import;
mod memory;
mod registry;
pub mod runtime;
mod verify;

pub use composer::{ComposeRequest, HostSnapshot, LaunchPlan, RuntimeChoice, compose};
pub use error::{InferenceError, Result};
pub use import::{
    ImportOptions, ImportPhase, ImportProgress, STAGING_DIR, STALE_PARTIAL_AGE,
    cleanup_partial_imports, import_model, import_model_with, remove_stale_partials,
};
pub use memory::{DEFAULT_TARGET_CONTEXT, MIN_CONTEXT, estimate_memory, overhead_bytes};
pub use registry::{
    LICENSE_FILE, LicenseRecord, LocalModelId, MODEL_FILE, MemoryEstimate, ModelCapabilityRecord,
    ModelFormat, ModelRegistry, REGISTRY_DIR, REGISTRY_VERSION, RegistryEntry, RegistryListing,
    RegistryProblem, RemovedModel, SourceRecord,
};
pub use verify::{
    VERIFIED_CACHE_FILE, VerificationCache, VerificationMethod, Verified, verify_model,
    verify_model_with,
};
