use std::path::PathBuf;

use aidex_core::Timestamp;
use aidex_host::{GpuDevice, GpuStatus, GpuVendor, HostResources};

use super::*;
use crate::{
    LicenseRecord, SourceRecord,
    gguf::{self, GgufBuilder},
    import::{EntryFacts, entry_from_metadata},
    runtime::{InstalledRuntime, RuntimeManifest, RuntimeVariant},
};

const GIB: u64 = 1024 * 1024 * 1024;

/// A 0.5 GiB model shaped like Qwen2.5-Coder-0.5B (24 layers, GQA, 32k context).
fn model(size_bytes: u64) -> RegistryEntry {
    let bytes = GgufBuilder::chat_model("qwen2", "Test Coder")
        .u32("qwen2.context_length", 32_768)
        .u32("qwen2.block_count", 24)
        .u32("qwen2.embedding_length", 896)
        .u32("qwen2.attention.head_count", 14)
        .u32("qwen2.attention.head_count_kv", 2)
        .build();
    let metadata = gguf::parse(bytes.as_slice(), None).unwrap();
    entry_from_metadata(
        &LocalModelId::new("test-coder").unwrap(),
        &metadata,
        EntryFacts {
            file: "models/test-coder/model.gguf".into(),
            sha256: "0".repeat(64),
            size_bytes,
            display_name: None,
            license: LicenseRecord::default(),
            source: SourceRecord::default(),
        },
    )
}

fn runtime(platform: &str, variant: RuntimeVariant) -> InstalledRuntime {
    let name = format!("b11205-{variant}");
    InstalledRuntime {
        dir: PathBuf::from(format!("runtime/{platform}/llama/{name}")),
        relative_dir: format!("runtime/{platform}/llama/{name}"),
        manifest: RuntimeManifest {
            version: 1,
            catalog_id: format!("llama.cpp-b11205-{platform}-{variant}"),
            engine: "llama.cpp".into(),
            runtime_version: "b11205".into(),
            platform: platform.into(),
            variant,
            archive: "archive.zip".into(),
            archive_sha256: "1".repeat(64),
            url: "https://example.invalid/archive.zip".into(),
            source: "https://example.invalid".into(),
            license: "MIT".into(),
            server_binary: "llama-server.exe".into(),
            files: Vec::new(),
            tree_sha256: "2".repeat(64),
            installed_at: Timestamp::now(),
        },
    }
}

fn host(available: Option<u64>, gpu: GpuStatus) -> HostSnapshot {
    HostSnapshot {
        os: "windows".into(),
        arch: "x86_64".into(),
        platform: Some("windows-x64".into()),
        resources: HostResources {
            total_ram_bytes: Some(16 * GIB),
            available_ram_bytes: available,
            physical_cores: Some(14),
            logical_cores: Some(20),
            cpu_brand: Some("Test CPU".into()),
        },
        gpu,
    }
}

fn nvidia(vram: u64) -> GpuStatus {
    GpuStatus::Detected {
        source: "nvidia-smi".into(),
        devices: vec![GpuDevice {
            vendor: GpuVendor::Nvidia,
            name: "NVIDIA Test GPU".into(),
            vram_bytes: Some(vram),
            driver_version: None,
            unified_memory: false,
        }],
    }
}

fn mentions(plan: &LaunchPlan, text: &str) -> bool {
    plan.reasons.iter().any(|reason| reason.contains(text))
}

#[test]
fn cpu_fallback_is_explained_when_a_gpu_has_no_runtime() {
    let plan = compose(
        &model(GIB / 2),
        &host(Some(8 * GIB), nvidia(6 * GIB)),
        &[runtime("windows-x64", RuntimeVariant::Cpu)],
        &ComposeRequest::default(),
    )
    .unwrap();
    assert_eq!(plan.runtime.variant, RuntimeVariant::Cpu);
    assert_eq!(plan.gpu_layers, 0);
    assert!(
        mentions(
            &plan,
            "GPU detected (NVIDIA Test GPU) but no supported GPU runtime is installed"
        ),
        "{:?}",
        plan.reasons
    );
}

#[test]
fn cpu_fallback_is_explained_when_the_gpu_is_unknown_or_unprobed() {
    let cpu = [runtime("windows-x64", RuntimeVariant::Cpu)];
    let unprobed = compose(
        &model(GIB / 2),
        &host(Some(8 * GIB), GpuStatus::NotProbed),
        &cpu,
        &ComposeRequest::default(),
    )
    .unwrap();
    assert!(
        mentions(&unprobed, "GPU not probed"),
        "{:?}",
        unprobed.reasons
    );
    let unknown = compose(
        &model(GIB / 2),
        &host(
            Some(8 * GIB),
            GpuStatus::Unknown {
                reason: "nvidia-smi is not on PATH".into(),
            },
        ),
        &cpu,
        &ComposeRequest::default(),
    )
    .unwrap();
    assert!(mentions(
        &unknown,
        "GPU unknown (nvidia-smi is not on PATH)"
    ));
}

#[test]
fn gpu_runtimes_are_used_only_with_a_matching_device() {
    let installed = [
        runtime("windows-x64", RuntimeVariant::Cpu),
        runtime("windows-x64", RuntimeVariant::Cuda),
    ];
    let with_gpu = compose(
        &model(GIB / 2),
        &host(Some(8 * GIB), nvidia(6 * GIB)),
        &installed,
        &ComposeRequest::default(),
    )
    .unwrap();
    assert_eq!(with_gpu.runtime.variant, RuntimeVariant::Cuda);
    assert_eq!(with_gpu.gpu_layers, 25, "all 24 layers + output fit 6 GiB");
    assert!(with_gpu.estimated_vram_bytes > 0);
    assert_eq!(with_gpu.batch_size, 2048);

    let without_gpu = compose(
        &model(GIB / 2),
        &host(Some(8 * GIB), GpuStatus::NotProbed),
        &installed,
        &ComposeRequest::default(),
    )
    .unwrap();
    assert_eq!(without_gpu.runtime.variant, RuntimeVariant::Cpu);
    assert!(mentions(
        &without_gpu,
        "skipped llama.cpp-b11205-windows-x64-cuda"
    ));

    let cpu_only = compose(
        &model(GIB / 2),
        &host(Some(8 * GIB), nvidia(6 * GIB)),
        &installed,
        &ComposeRequest {
            cpu_only: true,
            ..ComposeRequest::default()
        },
    )
    .unwrap();
    assert_eq!(cpu_only.runtime.variant, RuntimeVariant::Cpu);
    assert!(mentions(&cpu_only, "disabled by request"));

    let small_vram = compose(
        &model(4 * GIB),
        &host(Some(12 * GIB), nvidia(GIB + GIB / 2)),
        &installed,
        &ComposeRequest::default(),
    )
    .unwrap();
    assert!(small_vram.gpu_layers > 0 && small_vram.gpu_layers < 25);

    let only_cuda = compose(
        &model(GIB / 2),
        &host(Some(8 * GIB), GpuStatus::NotProbed),
        &[runtime("windows-x64", RuntimeVariant::Cuda)],
        &ComposeRequest::default(),
    )
    .unwrap_err();
    assert_eq!(only_cuda.kind(), "runtime_not_installed");
}

#[test]
fn runtimes_for_other_platforms_are_never_selected() {
    let error = compose(
        &model(GIB / 2),
        &host(Some(8 * GIB), GpuStatus::NotProbed),
        &[
            runtime("linux-x64", RuntimeVariant::Cpu),
            runtime("macos-arm64", RuntimeVariant::Metal),
        ],
        &ComposeRequest::default(),
    )
    .unwrap_err();
    let InferenceError::RuntimeNotInstalled { platform, reason } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(platform, "windows-x64");
    assert!(reason.contains("2 runtime(s) for other platforms"));
    assert!(error.advice().unwrap().contains("aidex runtime install"));
}

#[test]
fn unsupported_platforms_fail_clearly() {
    let mut snapshot = host(Some(8 * GIB), GpuStatus::NotProbed);
    snapshot.platform = None;
    snapshot.os = "freebsd".into();
    let error = compose(
        &model(GIB / 2),
        &snapshot,
        &[runtime("windows-x64", RuntimeVariant::Cpu)],
        &ComposeRequest::default(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), "unsupported_platform");
    assert!(error.to_string().contains("freebsd"));
}

#[test]
fn context_is_sized_to_fit_available_ram() {
    let entry = model(GIB / 2);
    let cpu = [runtime("windows-x64", RuntimeVariant::Cpu)];
    let roomy = compose(
        &entry,
        &host(Some(8 * GIB), GpuStatus::NotProbed),
        &cpu,
        &ComposeRequest::default(),
    )
    .unwrap();
    assert_eq!(roomy.ctx_size, DEFAULT_TARGET_CONTEXT);
    assert_eq!(roomy.threads, 14);
    assert!(roomy.mmap);

    // Model 0.5 GiB + overhead + 0.8 GiB reserve leave room for a few thousand tokens.
    let fixed = entry.size_bytes + entry.memory.overhead_bytes + (16 * GIB / 20);
    let tight = fixed + entry.memory.kv_bytes_per_token * 5000;
    let shrunk = compose(
        &entry,
        &host(Some(tight), GpuStatus::NotProbed),
        &cpu,
        &ComposeRequest::default(),
    )
    .unwrap();
    assert_eq!(
        shrunk.ctx_size, 4864,
        "5000 tokens rounded down to a multiple of 256"
    );
    assert!(mentions(&shrunk, "context reduced from 16384 to 4864"));

    let explicit = compose(
        &entry,
        &host(Some(tight), GpuStatus::NotProbed),
        &cpu,
        &ComposeRequest {
            context: Some(8192),
            ..ComposeRequest::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        explicit.kind(),
        "insufficient_memory",
        "explicit contexts are never shrunk"
    );

    let too_long = compose(
        &entry,
        &host(Some(8 * GIB), GpuStatus::NotProbed),
        &cpu,
        &ComposeRequest {
            context: Some(65_536),
            ..ComposeRequest::default()
        },
    )
    .unwrap_err();
    assert_eq!(too_long.kind(), "invalid_argument");

    let unknown = compose(
        &entry,
        &host(None, GpuStatus::NotProbed),
        &cpu,
        &ComposeRequest::default(),
    )
    .unwrap();
    assert_eq!(unknown.ctx_size, 4096);
    assert!(mentions(&unknown, "available RAM is unknown"));
}

#[test]
fn insufficient_ram_is_refused_with_an_explanation() {
    let error = compose(
        &model(6 * GIB),
        &host(Some(4 * GIB), GpuStatus::NotProbed),
        &[runtime("windows-x64", RuntimeVariant::Cpu)],
        &ComposeRequest::default(),
    )
    .unwrap_err();
    let InferenceError::InsufficientMemory {
        required_bytes,
        available_bytes,
        explanation,
    } = &error
    else {
        panic!("{error:?}");
    };
    assert!(required_bytes > available_bytes);
    assert!(
        explanation.contains("test-coder needs about"),
        "{explanation}"
    );
    assert!(
        explanation.contains("4.0 GiB is available"),
        "{explanation}"
    );
    assert!(error.advice().unwrap().contains("--context"));
}

#[test]
fn memory_estimates_are_recorded_at_import() {
    let entry = model(GIB / 2);
    assert_eq!(
        entry.memory.kv_bytes_per_token, 12_288,
        "Qwen2.5-0.5B shape"
    );
    assert_eq!(entry.memory.min_context, MIN_CONTEXT);
    assert_eq!(entry.memory.recommended_context, DEFAULT_TARGET_CONTEXT);
    assert!(entry.memory.min_ram_bytes > entry.size_bytes);
    assert!(entry.memory.recommended_ram_bytes > entry.memory.min_ram_bytes);
}
