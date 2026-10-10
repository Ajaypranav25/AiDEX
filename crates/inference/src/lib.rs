//! Portable local GGUF inference (spec §10, §11, §46, §66, §67, §86, §96, §97).
//!
//! * [`gguf`] reads model metadata without loading tensors.
//! * [`import_model`] copies a GGUF file onto the drive and registers it in the
//!   [`ModelRegistry`]; [`verify_model`] checks it before use.
//! * [`runtime`] installs pinned llama.cpp release archives and verifies them
//!   before every launch.
//! * [`compose`] turns a model, the host, and installed runtimes into a
//!   [`LaunchPlan`] with explanations.
//! * [`LocalInference`] runs managed `llama-server` processes through
//!   `aidex-sandbox`, schedules residency, and backs [`LocalLlamaProvider`].

mod benchmark;
pub mod composer;
mod error;
mod fsutil;
pub mod gguf;
mod import;
mod memory;
mod provider;
mod registry;
pub mod runtime;
mod scheduler;
mod server;
mod service;
mod verify;

pub use benchmark::{
    BENCHMARK_PROMPT, BENCHMARK_TOKENS, BENCHMARKS_FILE, BenchmarkReport,
    RECOMMENDED_TOKENS_PER_SECOND, host_fingerprint,
};
pub use composer::{ComposeRequest, HostSnapshot, LaunchPlan, RuntimeChoice, compose};
pub use error::{InferenceError, Result};
pub use import::{
    ImportOptions, ImportPhase, ImportProgress, STAGING_DIR, STALE_PARTIAL_AGE,
    cleanup_partial_imports, import_model, import_model_with, remove_stale_partials,
};
pub use memory::{DEFAULT_TARGET_CONTEXT, MIN_CONTEXT, estimate_memory, overhead_bytes};
pub use provider::LocalLlamaProvider;
pub use registry::{
    LICENSE_FILE, LicenseRecord, LocalModelId, MODEL_FILE, MemoryEstimate, ModelCapabilityRecord,
    ModelFormat, ModelRegistry, REGISTRY_DIR, REGISTRY_VERSION, RegistryEntry, RegistryListing,
    RegistryProblem, RemovedModel, SourceRecord,
};
pub use scheduler::{LoadedSummary, SchedulerPolicy};
pub use server::{API_KEY_ENV, CompletionTimings, ManagedServer, ServerConfig, ServerInfo};
pub use service::{
    InferenceOptions, LOCAL_PROVIDER_ID, LOGS_DIR, LoadOptions, LoadReport, LocalInference,
    PlanReport, RuntimeInventory, SCRATCH_DIR,
};
pub use verify::{
    VERIFIED_CACHE_FILE, VerificationCache, VerificationMethod, Verified, verify_model,
    verify_model_with,
};
