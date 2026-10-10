# Inference validation

The default test suite needs no real model or runtime: `cargo test -p aidex-inference --features test-fixture` drives the managed path with a fixture HTTP server and synthetic GGUF files (see `crates/inference/README.md`).

## Documented test model

| Field | Value |
| --- | --- |
| Model | Qwen2.5-Coder-0.5B-Instruct, `qwen2.5-coder-0.5b-instruct-q4_k_m.gguf` |
| Source | <https://huggingface.co/Qwen/Qwen2.5-Coder-0.5B-Instruct-GGUF> at revision `ebb2015119c907b064c512bf053e945850b5875f` |
| Licence | Apache-2.0 (the repository's `LICENSE`; keep it with the model) |
| Size / SHA-256 | 491,400,064 bytes, `1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32` (AiDEX model id `m-1d9614638d18024d0fbb36575a15f1302a3adf044df10345688ec4f6e1c4ff32`) |
| Metadata | `qwen2`, Q4_K_M, 24 layers, 896 embedding, 14/2 attention heads, 32,768 training context |

## Real-runtime test (ignored by default)

```sh
cargo build -p aidex-sandbox --bin aidex-sandbox-exec
AIDEX_TEST_LLAMA_ZIP=/path/to/llama-b11205-bin-win-cpu-x64.zip \
AIDEX_TEST_GGUF=/path/to/qwen2.5-coder-0.5b-instruct-q4_k_m.gguf \
cargo test -p aidex-inference --test real_llama -- --ignored --nocapture
```

It checks the archive digest, builds the distribution manifest, installs the runtime and imports the model into a temporary drive, runs the opt-in GPU probe, starts with `auto` settings while cloud networking is offline, completes a code prompt and a JSON-schema prompt over loopback, and stops.

Result on the validation host (Windows 11, i7-13650HX with 14 physical / 20 logical cores, 16 GiB RAM with 3.9 GiB available while other builds ran, RTX 3050 6 GiB detected; debug build), 2026-10-11:

- runtime install 6.2 s (51 files verified and copied); model import 13.8 s (copy plus SHA-256 of 491 MB in a debug build);
- plan: CPU backend — "the installed llama.cpp runtime is CPU-only (GPU detected by nvidia-smi: NVIDIA GeForce RTX 3050 6GB Laptop GPU (6144 MiB VRAM))", 14 threads (physical cores), context 4096 ("fits 3.9 GiB of available RAM"), estimated 964 MiB;
- start to ready 15.1 s (includes the fresh SHA-256 of the model and runtime that every start performs, plus model load);
- chat: 51 prompt + 30 completion tokens in 1.23 s, first token after 0.28 s, about 31.7 tokens/s generation measured at the client; the JSON-schema answer parsed (`{"age": 36, "name": "Ada"}`).

A temporary directory was used as the drive, not removable media; the code path is the same on a drive root.
