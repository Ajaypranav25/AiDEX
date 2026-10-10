# Inference validation

The default test suite never needs a real model or runtime: `crates/inference/tests` drive the full managed path against `fake-llama-server` (a test-only binary that speaks the subset of the llama-server API AiDEX uses) packaged into a synthetic runtime archive, plus synthetic GGUF files.

## Documented test model

| Field | Value |
| --- | --- |
| Model | Qwen2.5-Coder-0.5B-Instruct, `qwen2.5-coder-0.5b-instruct-q4_k_m.gguf` |
| Source | <https://huggingface.co/Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF> at revision `ebb2015119c907b064c512bf053e945850b5875f` |
| Licence | Apache-2.0 (the repository's `LICENSE`; import it with `--license-file`) |
| Size / SHA-256 | 491,400,064 bytes, `1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32` |
| Metadata | `qwen2`, Q4_K_M, 24 layers, 896 embedding, 14/2 attention heads, 151,936 tokens, 32,768 training context |

## Real-runtime test (ignored by default)

```sh
AIDEX_TEST_LLAMA_ZIP=/path/to/llama-b11205-bin-win-cpu-x64.zip \
AIDEX_TEST_GGUF=/path/to/qwen2.5-coder-0.5b-instruct-q4_k_m.gguf \
cargo test -p aidex-inference -- --ignored --nocapture
```

It installs the pinned runtime and imports the model (with the `LICENSE` next to the GGUF, if present) into a temporary drive, verifies the model, plans with the GPU probe, loads through the sandbox, completes a code prompt and a JSON-schema prompt over loopback with `--offline`, benchmarks, and unloads. `staged_real_model_header_parses_and_tensor_sizes_match_the_file` also checks the GGUF parser against the real file (computed tensor end equals the file size).

Result on the validation host (Windows 11, i7-13650HX 14 cores, 16 GiB RAM with about 2 GiB free while other builds ran, RTX 3050 detected but only the CPU runtime installed, debug build), 2026-10-10:

- install 1.7 s; import (copy + hash + verify) 16.3 s and forced verification 14.0 s (debug-build SHA-256 of 491 MB);
- plan: CPU runtime with the reason "GPU detected (NVIDIA GeForce RTX 3050 6GB Laptop GPU) but no supported GPU runtime is installed", 14 threads, context 16,384, estimated 1,170 MiB RAM;
- server ready after 4.9 s (cold cache; 1.0–1.9 s warm);
- chat: 51 prompt + 30 completion tokens in 1.55 s, first token after 0.50 s; JSON-schema output parsed;
- benchmark: prompt 310.6 tok/s, generation 36.6 tok/s, server RAM 695 MiB, recommended.

The same flow through the CLI and a detached daemon with offline mode on (`aidex runtime install`, `aidex model import ... --license-file`, `aidex runtime plan --probe-gpu`, `aidex model run`, `aidex model benchmark`, `aidex model unload`, `aidex daemon stop`) completed with the server stopped afterwards. Temporary directories were used, not a removable drive; the code path is identical on a drive root.
