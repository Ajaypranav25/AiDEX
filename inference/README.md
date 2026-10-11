# Inference

Repository-side records for AiDEX's managed local inference, which is implemented in [`crates/inference`](../crates/inference/README.md) (`aidex-inference`, served by the daemon as `inference.*` and by the CLI as `aidex inference`):

- [`llama-cpp/`](llama-cpp/README.md) — the pinned llama.cpp release, its licences, source, and official archive digests, and how a verified distribution directory is produced from it.
- [`presets/`](presets/README.md) — how start settings are chosen (no static presets are shipped).
- [`tests/`](tests/README.md) — the documented validation model, its licence and source, and the real-runtime test with measured results.
- `build/`, `patches/` — reserved; AiDEX runs upstream release binaries unmodified.

No model weights, runtime binaries, or archives are committed.
