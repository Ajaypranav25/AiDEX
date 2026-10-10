# Host capability profile

`aidex-host::HostProfile::current()` returns a versioned snapshot (`version: 1`) of process and host facts. The profile serializes with `serde`; JSON output follows the struct field order and preserves the `available`, `unavailable`, and `unknown` probe states.

Tool discovery is bounded to the first 256 PATH entries and checks filesystem metadata only. It never invokes discovered executables. An `available` result proves only that a matching file was found, not that it is executable, functional, or suitable for a workload. Supported checks cover platform shells (`powershell`/`cmd` on Windows; `sh`/`bash` on Unix), Git, `rustc`/`cargo`, and PATH-visible local inference tools (`ollama`, `llama-server`, and `llama-cli`). If the PATH search is truncated or cannot be inspected, the result is `unknown`, not `unavailable`.

CPU count and process paths are best-effort standard-library facts. RAM is read from Linux `/proc/meminfo` only; other platforms report no RAM value. The temporary and current directories are checked through read-only metadata. Write permission, network state, GPU presence, and sandbox status remain `unknown`: the API does not attempt writes, network connections, shell commands, or unsafe hardware probes.
