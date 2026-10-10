//! Memory estimates recorded at import and used by the composer: model size,
//! runtime compute buffers, and the f16 KV cache per context token.

use crate::{MemoryEstimate, gguf::GgufMetadata};

pub(crate) const MIB: u64 = 1024 * 1024;
pub(crate) const GIB: u64 = 1024 * MIB;

/// Smallest context worth starting a server for.
pub const MIN_CONTEXT: u32 = 2048;
/// Context used when the caller does not ask for one.
pub const DEFAULT_TARGET_CONTEXT: u32 = 16_384;
/// Context used when available RAM is unknown.
pub(crate) const UNKNOWN_RAM_CONTEXT: u32 = 4096;
/// Contexts are rounded down to a multiple of this.
pub(crate) const CONTEXT_STEP: u32 = 256;
/// llama-server process baseline (threads, HTTP, tokenizer) assumed per model.
const RUNTIME_BASELINE: u64 = 192 * MIB;
/// Physical batch size (`--ubatch-size`).
pub(crate) const UBATCH: u64 = 512;
/// Vocabulary assumed when the model does not state one.
const ASSUMED_VOCAB: u64 = 128_000;

/// Runtime overhead estimate: baseline plus compute buffers (output logits for
/// one physical batch and activations).
pub fn overhead_bytes(vocab: Option<u64>, embedding: Option<u64>) -> u64 {
    let vocab = vocab.unwrap_or(ASSUMED_VOCAB);
    let embedding = embedding.unwrap_or(4096);
    RUNTIME_BASELINE
        .saturating_add(vocab.saturating_mul(UBATCH).saturating_mul(4))
        .saturating_add(embedding.saturating_mul(UBATCH).saturating_mul(4 * 12))
}

/// Registry memory estimates for a model of `size_bytes` with `metadata`.
pub fn estimate_memory(metadata: &GgufMetadata, size_bytes: u64) -> MemoryEstimate {
    let train = clamp_u32(
        metadata
            .context_length
            .unwrap_or(u64::from(UNKNOWN_RAM_CONTEXT)),
    );
    let min_context = MIN_CONTEXT.min(train).max(1);
    let recommended_context = DEFAULT_TARGET_CONTEXT.min(train).max(min_context);
    let kv = metadata
        .kv_bytes_per_token()
        .unwrap_or_else(|| metadata.embedding_length.unwrap_or(4096) * 2 * 2 * 32);
    let overhead = overhead_bytes(metadata.vocab_size, metadata.embedding_length);
    let need = |context: u32| {
        size_bytes
            .saturating_add(overhead)
            .saturating_add(kv.saturating_mul(u64::from(context)))
    };
    MemoryEstimate {
        min_ram_bytes: need(min_context),
        recommended_ram_bytes: need(recommended_context),
        min_context,
        recommended_context,
        kv_bytes_per_token: kv,
        overhead_bytes: overhead,
    }
}

pub(crate) fn clamp_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
