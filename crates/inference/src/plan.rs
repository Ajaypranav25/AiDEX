//! Host-aware launch settings (spec §10.4, §11): resolve `auto` start options
//! from the model, the installed runtime, and the host's memory, physical
//! cores, and (opt-in) GPU probe, recording the reason for every choice —
//! including why the CPU backend was used.

use aidex_host::{GpuStatus, HostResources};
use serde::{Deserialize, Serialize};

use crate::{Backend, InferenceError, StartOptions, gguf::Shape};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
/// Context used when the caller does not choose one (and the model allows it).
pub(crate) const DEFAULT_CONTEXT: u32 = 4096;
/// Smallest context the planner shrinks a default to.
const MIN_PLANNED_CONTEXT: u32 = 2048;
/// Contexts are rounded down to a multiple of this.
const CONTEXT_STEP: u32 = 256;
/// Threads used at most when chosen automatically.
const MAX_AUTO_THREADS: usize = 16;
/// llama-server baseline plus compute buffers for one 512-token batch with a
/// large (128k) vocabulary.
const RUNTIME_OVERHEAD: u64 = 448 * MIB;
/// RAM left for the operating system and AiDEX itself.
const RESERVE: u64 = 256 * MIB;

/// The settings a start resolved to, with the facts and reasons behind them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchPlan {
    pub backend: Backend,
    pub threads: u32,
    pub context_window: u32,
    pub gpu_layers: u32,
    /// Model + runtime buffers + KV cache for `context_window`, when estimable.
    pub estimated_ram_bytes: Option<u64>,
    pub total_ram_bytes: Option<u64>,
    pub available_ram_bytes: Option<u64>,
    pub physical_cores: Option<usize>,
    pub logical_cores: Option<usize>,
    /// The GPU probe result the plan saw (`not_probed` unless requested).
    pub gpu: GpuStatus,
    pub reasons: Vec<String>,
}

/// What the planner knows about the model.
pub(crate) struct ModelFacts {
    pub size_bytes: u64,
    pub context_limit: Option<u32>,
    pub shape: Shape,
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / GIB as f64)
}

/// Resolve `options` for this host. GPU requests are refused before this runs.
pub(crate) fn compose(
    options: &StartOptions,
    model: &ModelFacts,
    runtime_gpu_supported: bool,
    host: &HostResources,
    gpu: &GpuStatus,
) -> Result<LaunchPlan, InferenceError> {
    let mut reasons = Vec::new();

    // Backend: CPU until a GPU runtime is proven on this host.
    match options.backend {
        Backend::Cpu => reasons.push("CPU backend requested".into()),
        Backend::Gpu => {
            return Err(InferenceError::Unavailable(
                "GPU backend cannot be proven available; select auto or CPU".into(),
            ));
        }
        Backend::Auto => {
            let gpu_note = match gpu {
                GpuStatus::NotProbed => "GPU devices were not probed (the probe is opt-in)".into(),
                other => other.summary(),
            };
            reasons.push(if runtime_gpu_supported {
                format!(
                    "auto selected the CPU backend: GPU offload is not proven for this runtime and host ({gpu_note})"
                )
            } else {
                format!(
                    "auto selected the CPU backend: the installed llama.cpp runtime is CPU-only ({gpu_note})"
                )
            });
        }
    }

    // Threads: physical cores, as llama.cpp recommends for generation.
    let threads = match options.threads {
        Some(threads) => {
            reasons.push(format!("{threads} threads (requested)"));
            threads
        }
        None => match host.physical_cores {
            Some(cores) => {
                let threads = cores.clamp(1, MAX_AUTO_THREADS);
                reasons.push(if threads < cores {
                    format!("{threads} threads (physical cores, capped at {MAX_AUTO_THREADS})")
                } else {
                    format!("{threads} threads (physical cores)")
                });
                threads as u32
            }
            None => {
                let threads = host.logical_cores.unwrap_or(1).clamp(1, 8);
                reasons.push(format!(
                    "{threads} threads (logical CPUs; the physical core count is unknown)"
                ));
                threads as u32
            }
        },
    };
    if !(1..=256).contains(&threads) {
        return Err(InferenceError::Invalid("threads must be 1-256".into()));
    }

    // Context: within the model limit and sized to memory.
    let limit = model.context_limit;
    let requested = options.context_window;
    let target = requested.unwrap_or_else(|| limit.unwrap_or(DEFAULT_CONTEXT).min(DEFAULT_CONTEXT));
    if !(512..=262_144).contains(&target) || limit.is_some_and(|limit| target > limit) {
        return Err(InferenceError::Invalid(
            "context window must be 512-262144 and within model metadata limit".into(),
        ));
    }
    let kv = model.shape.kv_bytes_per_token();
    let fixed = model.size_bytes.saturating_add(RUNTIME_OVERHEAD);
    let need =
        |context: u32| kv.map(|kv| fixed.saturating_add(kv.saturating_mul(u64::from(context))));
    let mut context = target;
    match (kv, host.available_ram_bytes, host.total_ram_bytes) {
        (None, ..) => reasons.push(format!(
            "context {context} tokens; the model's attention shape is unknown, so memory was not sized"
        )),
        (Some(kv), available, total) => {
            let floor = if requested.is_some() {
                target
            } else {
                MIN_PLANNED_CONTEXT.min(target)
            };
            if let (Some(total), Some(required)) = (total, need(floor))
                && required > total
            {
                    return Err(InferenceError::Unavailable(format!(
                        "not enough memory: the model needs about {} for {floor} tokens of context (model {}, runtime buffers {}, KV cache {}) but this host has {} of RAM",
                        gib(required),
                        gib(model.size_bytes),
                        gib(RUNTIME_OVERHEAD),
                        gib(kv.saturating_mul(u64::from(floor))),
                        gib(total)
                    )));
            }
            match available {
                Some(available) if requested.is_none() => {
                    let budget = available.saturating_sub(RESERVE).saturating_sub(fixed);
                    let fit = u32::try_from(budget / kv.max(1)).unwrap_or(u32::MAX);
                    if fit < target {
                        let mut shrunk = fit.max(floor);
                        if shrunk > CONTEXT_STEP {
                            shrunk -= shrunk % CONTEXT_STEP;
                        }
                        context = shrunk.max(floor).min(target);
                        reasons.push(format!(
                            "context reduced from {target} to {context} tokens to fit {} of available RAM",
                            gib(available)
                        ));
                    } else {
                        reasons.push(format!(
                            "context {context} tokens fits {} of available RAM",
                            gib(available)
                        ));
                    }
                }
                Some(_) => reasons.push(format!("context {context} tokens (requested)")),
                None => reasons.push(format!(
                    "context {context} tokens; available RAM is unknown"
                )),
            }
            if let (Some(available), Some(required)) = (available, need(context))
                && required.saturating_add(RESERVE) > available
            {
                    reasons.push(format!(
                        "estimated {} exceeds the {} of RAM available now; the system may page the memory-mapped model",
                        gib(required),
                        gib(available)
                    ));
            }
        }
    }

    Ok(LaunchPlan {
        backend: Backend::Cpu,
        threads,
        context_window: context,
        gpu_layers: 0,
        estimated_ram_bytes: need(context),
        total_ram_bytes: host.total_ram_bytes,
        available_ram_bytes: host.available_ram_bytes,
        physical_cores: host.physical_cores,
        logical_cores: host.logical_cores,
        gpu: gpu.clone(),
        reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aidex_host::{GpuDevice, GpuVendor};

    fn options(backend: Backend) -> StartOptions {
        StartOptions {
            model_id: "m-test".into(),
            backend,
            context_window: None,
            threads: None,
        }
    }

    /// A Qwen2.5-0.5B-shaped model: 24 layers, 896 wide, 14/2 heads (12 KiB/token).
    fn qwen(size_bytes: u64) -> ModelFacts {
        ModelFacts {
            size_bytes,
            context_limit: Some(32_768),
            shape: Shape {
                block_count: Some(24),
                embedding_length: Some(896),
                head_count: Some(14),
                head_count_kv: Some(2),
            },
        }
    }

    fn host(available: Option<u64>) -> HostResources {
        HostResources {
            total_ram_bytes: Some(16 * GIB),
            available_ram_bytes: available,
            physical_cores: Some(14),
            logical_cores: Some(20),
            cpu_brand: Some("test".into()),
        }
    }

    fn nvidia() -> GpuStatus {
        GpuStatus::Detected {
            source: "nvidia-smi".into(),
            devices: vec![GpuDevice {
                vendor: GpuVendor::Nvidia,
                name: "NVIDIA Test".into(),
                vram_bytes: Some(6 * GIB),
                driver_version: None,
                unified_memory: false,
            }],
        }
    }

    #[test]
    fn auto_explains_the_cpu_fallback() {
        let plan = compose(
            &options(Backend::Auto),
            &qwen(GIB / 2),
            false,
            &host(Some(8 * GIB)),
            &nvidia(),
        )
        .unwrap();
        assert_eq!((plan.backend, plan.gpu_layers), (Backend::Cpu, 0));
        assert!(
            plan.reasons[0].contains("runtime is CPU-only")
                && plan.reasons[0].contains("NVIDIA Test"),
            "{:?}",
            plan.reasons
        );
        let unprobed = compose(
            &options(Backend::Auto),
            &qwen(GIB / 2),
            true,
            &host(Some(8 * GIB)),
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert!(unprobed.reasons[0].contains("not proven"));
        assert!(unprobed.reasons[0].contains("not probed"));
        assert!(
            compose(
                &options(Backend::Gpu),
                &qwen(1),
                false,
                &host(None),
                &nvidia()
            )
            .is_err()
        );
    }

    #[test]
    fn threads_follow_physical_cores() {
        let plan = compose(
            &options(Backend::Cpu),
            &qwen(1),
            false,
            &host(Some(8 * GIB)),
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert_eq!(plan.threads, 14);
        let mut big = host(Some(8 * GIB));
        big.physical_cores = Some(64);
        let plan = compose(
            &options(Backend::Cpu),
            &qwen(1),
            false,
            &big,
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert_eq!(plan.threads, 16);
        let mut unknown = host(Some(8 * GIB));
        unknown.physical_cores = None;
        let plan = compose(
            &options(Backend::Cpu),
            &qwen(1),
            false,
            &unknown,
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert_eq!(plan.threads, 8);
        assert!(
            plan.reasons
                .iter()
                .any(|r| r.contains("physical core count is unknown"))
        );
    }

    #[test]
    fn default_context_shrinks_to_available_ram() {
        let roomy = compose(
            &options(Backend::Auto),
            &qwen(GIB / 2),
            false,
            &host(Some(8 * GIB)),
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert_eq!(roomy.context_window, DEFAULT_CONTEXT);
        assert_eq!(
            roomy.estimated_ram_bytes,
            Some(GIB / 2 + RUNTIME_OVERHEAD + 12_288 * u64::from(DEFAULT_CONTEXT))
        );
        // Room for 3000 tokens of KV cache after the model, buffers, and reserve.
        let tight = GIB / 2 + RUNTIME_OVERHEAD + RESERVE + 12_288 * 3000;
        let shrunk = compose(
            &options(Backend::Auto),
            &qwen(GIB / 2),
            false,
            &host(Some(tight)),
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert_eq!(shrunk.context_window, 2816);
        assert!(
            shrunk
                .reasons
                .iter()
                .any(|r| r.contains("context reduced from 4096 to 2816"))
        );
        // Below the floor it starts at 2048 and says the OS may page.
        let starved = compose(
            &options(Backend::Auto),
            &qwen(GIB / 2),
            false,
            &host(Some(GIB / 4)),
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert_eq!(starved.context_window, 2048);
        assert!(starved.reasons.iter().any(|r| r.contains("may page")));
    }

    #[test]
    fn models_larger_than_the_host_are_refused_with_numbers() {
        let error = compose(
            &options(Backend::Auto),
            &qwen(20 * GIB),
            false,
            &host(Some(8 * GIB)),
            &GpuStatus::NotProbed,
        )
        .unwrap_err();
        let InferenceError::Unavailable(message) = &error else {
            panic!("{error:?}");
        };
        assert!(
            message.contains("not enough memory") && message.contains("16.0 GiB of RAM"),
            "{message}"
        );
    }

    #[test]
    fn explicit_settings_are_validated_not_shrunk() {
        let mut requested = options(Backend::Cpu);
        requested.context_window = Some(8192);
        requested.threads = Some(3);
        let plan = compose(
            &requested,
            &qwen(GIB / 2),
            false,
            &host(Some(GIB)),
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert_eq!((plan.context_window, plan.threads), (8192, 3));
        assert!(plan.reasons.iter().any(|r| r.contains("may page")));
        requested.context_window = Some(65_536);
        assert!(
            compose(
                &requested,
                &qwen(1),
                false,
                &host(None),
                &GpuStatus::NotProbed
            )
            .is_err()
        );
        let unshaped = ModelFacts {
            size_bytes: 1,
            context_limit: Some(2048),
            shape: Shape::default(),
        };
        let plan = compose(
            &options(Backend::Auto),
            &unshaped,
            false,
            &host(None),
            &GpuStatus::NotProbed,
        )
        .unwrap();
        assert_eq!(plan.context_window, 2048);
        assert!(
            plan.reasons
                .iter()
                .any(|r| r.contains("attention shape is unknown"))
        );
    }
}
