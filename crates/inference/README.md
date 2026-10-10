# Managed local inference

`InferenceService::open(DrivePaths, Arc<ProviderRegistry>, Arc<dyn EgressGate>, Sandbox)` uses the selected drive's model, runtime, scratch, and configuration directories. Opening a service does not launch a process. Use `import`, `install_runtime`, then `start`. The provider id is `local-llama`; the model id is `m-<SHA-256>`. The existing provider contract carries all chat requests, streaming events, and failures, including when cloud networking is offline.

The daemon adapters expose `inference.catalog`, `inference.import {source,name?}`, `inference.runtime.install {source?}`, `inference.start {model_id,backend?,context_window?,threads?}`, `inference.stop {}`, `inference.status {}`, `inference.model.remove {id}`, and `inference.cancel {}`. Parameters reject unknown fields. Source paths are absolute. Operations are synchronous; send cancellation on another daemon connection. Stop also cancels the active operation. Connection shutdown and daemon shutdown propagate cancellation while an operation is running.

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

Start binds only `127.0.0.1`, chooses an available ephemeral port, invokes argv directly through `aidex-sandbox`, and polls bounded `/health` requests until it reports `status: ok`. Only then does the service register the one loaded model. Stale provider handles fail once the process ends. Stop/drop terminates the tracked sandbox process tree and unregisters the provider. Runtime settings persist in `DrivePaths::config()/inference-settings.json`; opening the service never auto-starts it. Auto currently selects CPU using host thread availability; explicit GPU fails because device support has not been proven. Imported model tool capabilities are conservatively `none`; chat and plan modes use the same provider contract. Native agent mode requires a separately proven tool-capable model.

Status includes the actual sandbox controls and their enforcement/watchdog distinction. Requested memory is bounded at 16 GiB, output capture at 1 MiB per stream, and model-load readiness at 120 seconds. Brokered execution clears inherited credentials and tracks a killable process tree. It does not claim OS filesystem/network confinement. The credential-free model API is reachable by other local applications. Readiness requests bypass proxies and refuse redirects.

Verification:

```text
cargo build -p aidex-sandbox --bin aidex-sandbox-exec -j 2
cargo test -p aidex-inference --features test-fixture -j 2
cargo test -p aidexd methods::inference --lib -j 2
cargo run -p aidex-inference --example prove_local -j 2 -- <verified-distribution> <GGUF-file>
```

The `test-fixture` feature builds an HTTP subprocess used only in tests. The proof example installs into a disposable temporary drive and uses the real model through `ProviderRegistry` while cloud networking is offline.
