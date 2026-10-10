# Inference

Repository-side material for AiDEX's local inference, implemented in [`crates/inference`](../crates/inference/README.md) (`aidex-inference`):

- [`runtimes.toml`](runtimes.toml) — the pinned runtime catalog compiled into AiDEX: upstream llama.cpp release archives with exact size, SHA-256, URL, licence, and (when measured) the hash of their extracted contents. Only archives listed here can be installed with `aidex runtime install`.
- [`llama-cpp/`](llama-cpp/README.md) — the pinned llama.cpp release, its licence and source, and the `llama-server` flags AiDEX depends on.
- [`presets/`](presets/README.md) — how launch settings are chosen (no static presets are shipped).
- [`tests/`](tests/README.md) — the documented validation model and how to run the real-runtime test.
- `build/`, `patches/` — reserved; AiDEX uses upstream release binaries unmodified.

No model weights, runtime binaries, or archives are committed. Users import models with `aidex model import <FILE>` and install runtimes with `aidex runtime install <ARCHIVE>`; both are hash-verified.
