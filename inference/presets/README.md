# Inference presets

AiDEX ships no static launch presets. Settings are composed per launch by `aidex_inference::compose` from the model's registry entry (size, layers, KV-cache bytes per token, training context), the host (`aidex-host`: available RAM, physical cores, and the opt-in GPU probe), and the installed runtimes. `aidex runtime plan <MODEL> [--probe-gpu] [--context N] [--cpu-only] [--no-mmap]` prints the plan and the reason for every choice, including why it fell back to CPU. Presets must never imply that model weights are included.
