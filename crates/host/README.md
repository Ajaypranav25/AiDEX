# Host capability profile

`aidex-host::HostProfile::current()` returns a versioned snapshot (`version: 1`; `physical_cpu_count` and `available_ram_bytes` are additive fields) of process and host facts. The profile serializes with `serde`; JSON output follows the struct field order and preserves the `available`, `unavailable`, and `unknown` probe states.

Tool discovery is bounded to the first 256 PATH entries and checks filesystem metadata only. It never invokes discovered executables. An `available` result proves only that a matching file was found, not that it is executable, functional, or suitable for a workload. Supported checks cover platform shells (`powershell`/`cmd` on Windows; `sh`/`bash` on Unix), Git, `rustc`/`cargo`, and PATH-visible local inference tools (`ollama`, `llama-server`, and `llama-cli`). If the PATH search is truncated or cannot be inspected, the result is `unknown`, not `unavailable`.

## Memory and CPU

`HostResources::current()` reads total and available RAM, physical and logical core counts, and the CPU brand through `sysinfo`'s safe API on Windows, Linux, and macOS (no programs are run). Values a platform does not report are `None`. `HostResources::compute_threads()` picks physical cores, else half the logical CPUs. `resident_memory_bytes(&[pid])` sums the resident memory of processes (used to report a model server's RAM). `HostProfile` copies these facts into `physical_cpu_count`, `ram_bytes`, and `available_ram_bytes`.

## GPU (opt-in)

`HostProfile::current()` reports `gpu: {"state": "unknown"}`. `HostProfile::current_with(&HostProbeOptions { gpu: Some(GpuProbeOptions::default()) })` runs the probe and reports presence (`available`/`unavailable`/`unknown`); `probe_gpus(&options)` returns the details: `GpuStatus::{NotProbed, Detected { source, devices: [GpuDevice { vendor, name, vram_bytes, driver_version, unified_memory }] }, NotDetected { source }, Unknown { reason }}`:

- Windows and Linux: `nvidia-smi --query-gpu=name,memory.total,driver_version --format=csv,noheader,nounits` located through the bounded PATH search, killed after the timeout (default 5 s), output capped at 64 KiB. This is the only probe that executes a program, which is why it is opt-in. AMD/Intel GPUs are not probed: a missing `nvidia-smi` is `unknown`, never "no GPU".
- Apple Silicon (macOS aarch64): the integrated GPU is reported from the platform (unified memory), without running anything.

## Limitations

The temporary and current directories are checked through read-only metadata. Write permission, network state, and sandbox status remain `unknown`: the API does not attempt writes, network connections, or shell commands. GPU VRAM is reported only for NVIDIA devices; drive throughput, thermal state, and power mode (spec §10.4) are not measured yet.
