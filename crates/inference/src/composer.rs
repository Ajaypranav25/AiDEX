//! Host-aware launch planning (spec §10.4, §11): pick a compatible installed
//! runtime, explain any CPU fallback, and size context, threads, batch, GPU
//! layers, and memory mapping to the host, refusing with an explanation when
//! the model cannot fit.

use aidex_host::{GpuDevice, GpuStatus, GpuVendor, HostResources};
use serde::{Deserialize, Serialize};

use crate::{
    InferenceError, LocalModelId, RegistryEntry, Result,
    runtime::{InstalledRuntime, RuntimeVariant},
};

use crate::memory::{
    CONTEXT_STEP, DEFAULT_TARGET_CONTEXT, GIB, MIB, MIN_CONTEXT, UBATCH, UNKNOWN_RAM_CONTEXT,
    clamp_u32,
};

/// VRAM kept free on a GPU.
const VRAM_RESERVE: u64 = 512 * MIB;
/// Threads at most.
const MAX_THREADS: usize = 32;

/// What the composer knows about the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostSnapshot {
    pub os: String,
    pub arch: String,
    /// `aidex_drive::platform_runtime_name()`; `None` on unsupported platforms.
    pub platform: Option<String>,
    pub resources: HostResources,
    pub gpu: GpuStatus,
}

impl HostSnapshot {
    /// The current host with the given GPU status (run the opt-in probe first
    /// to fill it; [`GpuStatus::NotProbed`] otherwise).
    pub fn current(gpu: GpuStatus) -> Self {
        Self {
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            platform: aidex_drive::platform_runtime_name().map(str::to_owned),
            resources: HostResources::current(),
            gpu,
        }
    }
}

/// Caller preferences for a plan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeRequest {
    /// Context tokens to require; refused (never silently shrunk) when it cannot fit.
    #[serde(default)]
    pub context: Option<u32>,
    /// Memory-map the model (default) or read it fully into RAM.
    #[serde(default)]
    pub mmap: Option<bool>,
    /// Never offload to a GPU even when a compatible runtime and GPU exist.
    #[serde(default)]
    pub cpu_only: bool,
}

/// The installed runtime a plan launches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeChoice {
    pub catalog_id: String,
    pub version: String,
    pub platform: String,
    pub variant: RuntimeVariant,
    /// Drive-relative install directory.
    pub dir: String,
}

/// Settings for one llama-server launch, with the reasons behind them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchPlan {
    pub model: LocalModelId,
    pub runtime: RuntimeChoice,
    pub threads: u32,
    pub batch_size: u32,
    pub ubatch_size: u32,
    pub ctx_size: u32,
    pub gpu_layers: u32,
    pub mmap: bool,
    pub estimated_ram_bytes: u64,
    pub estimated_vram_bytes: u64,
    pub reasons: Vec<String>,
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / GIB as f64)
}

fn mib(bytes: u64) -> String {
    format!("{} MiB", bytes / MIB)
}

fn device_names<'a>(devices: impl IntoIterator<Item = &'a GpuDevice>) -> String {
    devices
        .into_iter()
        .map(|device| device.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Which detected devices a GPU runtime variant can drive.
fn compatible_devices(variant: RuntimeVariant, gpu: &GpuStatus) -> Vec<&GpuDevice> {
    gpu.devices()
        .iter()
        .filter(|device| match variant {
            RuntimeVariant::Cpu => false,
            RuntimeVariant::Cuda => device.vendor == GpuVendor::Nvidia,
            RuntimeVariant::Metal => device.vendor == GpuVendor::Apple,
            RuntimeVariant::Vulkan => device.vendor != GpuVendor::Apple,
        })
        .collect()
}

fn variant_rank(variant: RuntimeVariant) -> u8 {
    match variant {
        RuntimeVariant::Cuda => 3,
        RuntimeVariant::Metal => 2,
        RuntimeVariant::Vulkan => 1,
        RuntimeVariant::Cpu => 0,
    }
}

fn cpu_fallback_reason(
    gpu: &GpuStatus,
    cpu_only: bool,
    gpu_runtimes: &[&InstalledRuntime],
) -> String {
    if cpu_only {
        return "GPU offload disabled by request; using the CPU runtime".into();
    }
    match gpu {
        GpuStatus::NotProbed => {
            "GPU not probed (the probe is opt-in); using the CPU runtime".into()
        }
        GpuStatus::Unknown { reason } => format!("GPU unknown ({reason}); using the CPU runtime"),
        GpuStatus::NotDetected { source } => {
            format!("no GPU reported by {source}; using the CPU runtime")
        }
        GpuStatus::Detected { devices, .. } => {
            let installed: Vec<&str> = gpu_runtimes
                .iter()
                .map(|runtime| runtime.manifest.variant.as_str())
                .collect();
            if installed.is_empty() {
                format!(
                    "GPU detected ({}) but no supported GPU runtime is installed; using the CPU runtime",
                    device_names(devices)
                )
            } else {
                format!(
                    "GPU detected ({}) but the installed GPU runtimes ({}) cannot drive it; using the CPU runtime",
                    device_names(devices),
                    installed.join(", ")
                )
            }
        }
    }
}

/// The preferred runtime: best variant, then the newest version.
fn newest(candidates: Vec<&InstalledRuntime>) -> Option<&InstalledRuntime> {
    candidates.into_iter().max_by(|a, b| {
        (
            variant_rank(a.manifest.variant),
            &a.manifest.runtime_version,
        )
            .cmp(&(
                variant_rank(b.manifest.variant),
                &b.manifest.runtime_version,
            ))
    })
}

/// Plan a launch of `model` on `host` with one of `runtimes`.
pub fn compose(
    model: &RegistryEntry,
    host: &HostSnapshot,
    runtimes: &[InstalledRuntime],
    request: &ComposeRequest,
) -> Result<LaunchPlan> {
    let platform = host
        .platform
        .clone()
        .ok_or_else(|| InferenceError::UnsupportedPlatform {
            os: host.os.clone(),
            arch: host.arch.clone(),
        })?;
    let mut reasons = Vec::new();

    // --- runtime selection: only this platform, GPU variants only for matching devices.
    let local: Vec<&InstalledRuntime> = runtimes
        .iter()
        .filter(|runtime| runtime.manifest.platform == platform)
        .collect();
    let foreign = runtimes.len() - local.len();
    if local.is_empty() {
        let mut reason = "no runtime is installed for this platform".to_owned();
        if foreign > 0 {
            reason.push_str(&format!(
                " ({foreign} runtime(s) for other platforms are on the drive and were ignored)"
            ));
        }
        return Err(InferenceError::RuntimeNotInstalled { platform, reason });
    }
    if foreign > 0 {
        reasons.push(format!(
            "ignored {foreign} runtime(s) built for other platforms"
        ));
    }
    let gpu_runtimes: Vec<&InstalledRuntime> = local
        .iter()
        .copied()
        .filter(|runtime| runtime.manifest.variant.is_gpu())
        .collect();
    for runtime in &gpu_runtimes {
        if compatible_devices(runtime.manifest.variant, &host.gpu).is_empty() {
            reasons.push(format!(
                "skipped {}: no compatible GPU detected",
                runtime.manifest.catalog_id
            ));
        }
    }
    let gpu_choice = if request.cpu_only {
        None
    } else {
        newest(
            gpu_runtimes
                .iter()
                .copied()
                .filter(|runtime| {
                    !compatible_devices(runtime.manifest.variant, &host.gpu).is_empty()
                })
                .collect(),
        )
    };
    let runtime = match gpu_choice {
        Some(runtime) => runtime,
        None => {
            let cpu = newest(
                local
                    .iter()
                    .copied()
                    .filter(|runtime| runtime.manifest.variant == RuntimeVariant::Cpu)
                    .collect(),
            );
            match cpu {
                Some(runtime) => {
                    reasons.push(cpu_fallback_reason(
                        &host.gpu,
                        request.cpu_only,
                        &gpu_runtimes,
                    ));
                    runtime
                }
                None => {
                    return Err(InferenceError::RuntimeNotInstalled {
                        platform,
                        reason: format!(
                            "only GPU runtimes are installed and none matches this host ({}); install the CPU runtime",
                            host.gpu.summary()
                        ),
                    });
                }
            }
        }
    };
    let variant = runtime.manifest.variant;
    if variant.is_gpu() {
        reasons.push(format!(
            "{} runtime {} selected for {}",
            variant,
            runtime.manifest.runtime_version,
            device_names(compatible_devices(variant, &host.gpu))
        ));
    } else {
        reasons.push(format!(
            "CPU runtime {} ({})",
            runtime.manifest.runtime_version, runtime.manifest.catalog_id
        ));
    }

    // --- GPU layers.
    let layers = model.block_count.unwrap_or(0).saturating_add(1);
    let model_bytes = model.size_bytes;
    let (gpu_layers, offload_fraction, vram) = if variant.is_gpu() {
        let devices = compatible_devices(variant, &host.gpu);
        let device = devices.first().copied();
        match device {
            Some(device) if device.unified_memory => {
                reasons.push("unified memory: every layer runs on the GPU".into());
                (layers, 0.0, 0)
            }
            Some(GpuDevice {
                vram_bytes: Some(vram),
                name,
                ..
            }) => {
                let per_layer = (model_bytes / u64::from(layers.max(1))).max(1);
                let usable = vram.saturating_sub(VRAM_RESERVE);
                let fit = clamp_u32(usable / per_layer).min(layers);
                reasons.push(format!(
                    "offloading {fit} of {layers} layers to {name} ({} of {} VRAM, estimate)",
                    mib(per_layer.saturating_mul(u64::from(fit))),
                    mib(*vram)
                ));
                (
                    fit,
                    f64::from(fit) / f64::from(layers.max(1)),
                    per_layer.saturating_mul(u64::from(fit)),
                )
            }
            Some(device) => {
                reasons.push(format!(
                    "VRAM of {} is unknown; no layers offloaded",
                    device.name
                ));
                (0, 0.0, 0)
            }
            None => (0, 0.0, 0),
        }
    } else {
        (0, 0.0, 0)
    };

    // --- context and memory.
    let kv = model.memory.kv_bytes_per_token.max(1);
    let overhead = model.memory.overhead_bytes;
    let model_ram = (model_bytes as f64 * (1.0 - offload_fraction)) as u64;
    let train = model.context_length.max(1);
    let target = match request.context {
        Some(requested) if requested > train => {
            return Err(InferenceError::InvalidArgument(format!(
                "context {requested} exceeds the model's training context of {train} tokens"
            )));
        }
        Some(requested) => requested.max(1),
        None => DEFAULT_TARGET_CONTEXT.min(train),
    };
    let minimum = MIN_CONTEXT.min(target);
    let total = host.resources.total_ram_bytes;
    let reserve = (512 * MIB).max(total.unwrap_or(0) / 20);
    let fixed = model_ram.saturating_add(overhead).saturating_add(reserve);
    let need = |context: u32| fixed.saturating_add(kv.saturating_mul(u64::from(context)));
    let ctx_size = match host.resources.available_ram_bytes {
        Some(available) => {
            let floor = if request.context.is_some() {
                target
            } else {
                minimum
            };
            if available < need(floor) {
                return Err(InferenceError::InsufficientMemory {
                    required_bytes: need(floor),
                    available_bytes: available,
                    explanation: format!(
                        "{} needs about {} (model {}, runtime buffers {}, KV cache for {floor} tokens {}, reserve {}) but {} is available",
                        model.id,
                        gib(need(floor)),
                        gib(model_ram),
                        mib(overhead),
                        mib(kv.saturating_mul(u64::from(floor))),
                        mib(reserve),
                        gib(available)
                    ),
                });
            }
            let fit = clamp_u32((available - fixed) / kv);
            let mut ctx = target.min(fit);
            if ctx > CONTEXT_STEP {
                ctx -= ctx % CONTEXT_STEP;
            }
            if ctx < target {
                reasons.push(format!(
                    "context reduced from {target} to {ctx} tokens to fit {} of available RAM",
                    gib(available)
                ));
            } else {
                reasons.push(format!(
                    "context {ctx} tokens (KV cache {}) fits {} of available RAM",
                    mib(kv.saturating_mul(u64::from(ctx))),
                    gib(available)
                ));
            }
            ctx
        }
        None => {
            let ctx = target.min(UNKNOWN_RAM_CONTEXT);
            reasons.push(format!(
                "available RAM is unknown; context limited to {ctx} tokens"
            ));
            ctx
        }
    };

    // --- threads, batch, memory mapping.
    let threads = host.resources.compute_threads().min(MAX_THREADS);
    reasons.push(match host.resources.physical_cores {
        Some(cores) if cores <= MAX_THREADS => format!("{threads} threads (physical cores)"),
        Some(_) => format!("{threads} threads (capped)"),
        None => format!("{threads} threads (physical core count unknown)"),
    });
    let batch_size = if variant.is_gpu() && gpu_layers > 0 {
        2048
    } else {
        512
    };
    let mmap = request.mmap.unwrap_or(true);
    reasons.push(if mmap {
        "model memory-mapped from the drive (pages load on first use)".into()
    } else {
        "model read fully into RAM (no memory mapping)".into()
    });

    Ok(LaunchPlan {
        model: model.id.clone(),
        runtime: RuntimeChoice {
            catalog_id: runtime.manifest.catalog_id.clone(),
            version: runtime.manifest.runtime_version.clone(),
            platform,
            variant,
            dir: runtime.relative_dir.clone(),
        },
        threads: u32::try_from(threads).unwrap_or(1),
        batch_size,
        ubatch_size: UBATCH as u32,
        ctx_size,
        gpu_layers,
        mmap,
        estimated_ram_bytes: model_ram
            .saturating_add(overhead)
            .saturating_add(kv.saturating_mul(u64::from(ctx_size))),
        estimated_vram_bytes: vram,
        reasons,
    })
}

#[cfg(test)]
mod tests;
