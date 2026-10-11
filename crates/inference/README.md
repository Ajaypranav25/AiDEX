# Managed local inference

`InferenceService::open(DrivePaths, Arc<ProviderRegistry>, Arc<dyn EgressGate>, Sandbox)` uses the selected drive's model, runtime, scratch, and configuration directories. Opening a service does not launch a process. Use `import`, `install_runtime`, then `start`. The provider id is `local-llama`; the model id is `m-<SHA-256>`. The existing provider contract carries all chat requests, streaming events, and failures, including when cloud networking is offline.

The daemon adapters expose `inference.host {probe_gpu?}`, `inference.catalog`, `inference.import {source,name?}`, `inference.runtime.install {source?}`, `inference.start {model_id,backend?,context_window?,threads?}`, `inference.stop {}`, `inference.status {}`, `inference.model.remove {id}`, and `inference.cancel {}`. Parameters reject unknown fields. Source paths are absolute. Operations are synchronous; send cancellation on another daemon connection. Stop also cancels the active operation. Connection shutdown and daemon shutdown propagate cancellation while an operation is running.

Models are copied into `DrivePaths::models()/m-<digest>/model.gguf`, with portable relative paths in `model.json`. Imports validate GGUF v2/v3 header, bounded metadata, context length, dimensions, quantization block sizes, complete tensor payload bounds, and non-overlapping offsets. Common F32/F16/BF16, integer, Q4/Q5/Q8, K and IQ formats are supported; unknown layouts fail closed. A `filename.gguf.sha256` sidecar is checked when present. Names and digest collisions refuse overwrite. Source files remain unchanged. Catalog availability is computed from the managed bytes, including a fresh SHA-256 check. A structurally valid GGUF may still be unusable for inference; successful `/health` is required before a loaded model is offered.

A runtime source is a local flat directory containing `runtime-manifest.json` and every declared file:

```json
{
  "version": 1,
  "release": "b11205",
  "platform": "windows-x64",
  "executable": "llama-server.exe",
  "gpu_supported": false,
  "files": [
    {
      "path": "llama-server.exe",
      "sha256": "<64 hex digits>",
      "size_bytes": 9216
    },
    {
      "path": "llama-server-impl.dll",
      "sha256": "<64 hex digits>",
      "size_bytes": 8916480
    }
  ]
}
```

Include all required native libraries in the manifest; the example lists only two files. Unlisted files, missing hashes, changed digests, links, junctions, unsafe names, unsupported platforms, insufficient space, and a previously installed destination refuse installation. The source manifest is the explicit local distribution's checksum authority; it provides integrity, not publisher authentication. Release packaging must acquire and verify the official archive digest before producing the manifest and must carry the corresponding licenses. No remote runtime or model is automatically downloaded or executed.

Omitting the source uses `AIDEX_LLAMA_DISTRIBUTION` when explicitly configured, otherwise `DrivePaths::runtime()/components/llama-cpp`. There are no developer-drive defaults. Runtime installation commits a verified staging directory into `DrivePaths::platform_runtime()/llama-cpp`. A running server's binaries are reverified before launch. Imports/runtime copies use 1 MiB buffers, explicit cancellation, available-space checks, fsync of files, and a final directory rename; interrupted staging never appears as a successful install. Configuration is written through a synced temporary file and rename. Process-local mutation locking rejects conflicts; the daemon's drive ownership remains the cross-process boundary. Path checks are point-in-time checks and cannot prevent another same-user process racing the filesystem.

Start binds only `127.0.0.1`, chooses an available ephemeral port, invokes argv directly through `aidex-sandbox`, and polls bounded `/health` requests until it reports `status: ok`. Only then does the service register the one loaded model. Stale provider handles fail once the process ends. Stop/drop terminates the tracked sandbox process tree and unregisters the provider. Runtime settings persist in `DrivePaths::config()/inference-settings.json`. Opening the service alone does not start it; Core calls `restore_saved()` on launch to verify and load the saved model on a background worker while IPC remains available. Auto currently selects CPU using host thread availability; explicit GPU fails because device support has not been proven. Imported model tool capabilities are conservatively `none`; chat and plan modes use the same provider contract. Native agent mode requires a separately proven tool-capable model.

Successful starts also save `local-inference.json` alongside the settings. Its versioned binding records the managed model ID and drive-relative model/runtime paths derived from the current manifest, so changing the drive letter requires no reimport. Existing settings restore without a binding file. Set the binding's `autoload` to `false` to skip restoration; a successful manual start enables it again. Binding paths must match derived locations and cannot redirect execution. `inference.status` exposes `starting`, `ready`, `failed`, or `stopped`, plus `model_name` and a safe `startup_error`. Failed restores preserve files and settings. Stop cancels queued loading and checksum work; terminal `shutdown()` also prevents subsequent launches.

Auto settings are host-aware: unless `context_window`/`threads` are given, `start` plans them from the model (size, training context, and the KV-cache size implied by its attention shape), the runtime manifest's `gpu_supported`, and `aidex-host` facts (total/available RAM and physical cores). Threads default to physical cores (at most 16); the context defaults to `min(4096, model limit)` and is reduced (to no less than 2048) to fit available RAM after the model, ~448 MiB of runtime buffers, and a 256 MiB reserve; a model that cannot fit 2048 tokens in total RAM is refused with the numbers, and an estimate above available RAM is reported as possible paging. The backend is CPU until GPU offload is proven; the plan says why (CPU-only runtime, or GPU not proven) and includes the GPU status from the opt-in probe (`InferenceService::host(probe_gpu)` / `inference.host {probe_gpu?}`, which may run `nvidia-smi`; the result is reused by later starts). `inference.status` returns the plan as `plan: {backend, threads, context_window, gpu_layers, estimated_ram_bytes, total_ram_bytes, available_ram_bytes, physical_cores, logical_cores, gpu, reasons}`. Windows renames of freshly written staging directories are retried briefly while antivirus scanners hold them.

Status includes the actual sandbox controls and their enforcement/watchdog distinction. Requested memory is bounded at 16 GiB, output capture at 1 MiB per stream, and model-load readiness at 120 seconds. Brokered execution clears inherited credentials and tracks a killable process tree. It does not claim OS filesystem/network confinement. The credential-free model API is reachable by other local applications. Readiness requests bypass proxies and refuse redirects.

Verification:

```text
cargo build -p aidex-sandbox --bin aidex-sandbox-exec -j 2
cargo test -p aidex-inference --features test-fixture -j 2
cargo test -p aidexd methods::inference --lib -j 2
cargo run -p aidex-inference --example prove_local -j 2 -- <verified-distribution> <GGUF-file>
AIDEX_TEST_LLAMA_ZIP=<llama-b11205-bin-win-cpu-x64.zip> AIDEX_TEST_GGUF=<model.gguf> cargo test -p aidex-inference --test real_llama -- --ignored --nocapture
```

The ignored `real_llama` test checks the archive against the b11205 release digest, builds the distribution manifest from it, and runs the documented model end to end; licences, sources, digests, and measured results are in [`inference/llama-cpp`](../../inference/llama-cpp/README.md) and [`inference/tests`](../../inference/tests/README.md). Windows x64 is the only validated platform.

The `test-fixture` feature builds an HTTP subprocess used only in tests. The proof example installs into a disposable temporary drive and uses the real model through `ProviderRegistry` while cloud networking is offline.
