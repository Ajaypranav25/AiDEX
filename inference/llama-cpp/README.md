# llama.cpp

AiDEX runs local GGUF models with the upstream [llama.cpp](https://github.com/ggml-org/llama.cpp) `llama-server`, unmodified, from pinned release archives. Nothing from llama.cpp is vendored or redistributed by this repository; users install an archive they downloaded (or copied) with `aidex runtime install <ARCHIVE>`, which accepts it only if its size and SHA-256 match [`../runtimes.toml`](../runtimes.toml).

## Pinned release

| Field | Value |
| --- | --- |
| Release | `b11205` — <https://github.com/ggml-org/llama.cpp/releases/tag/b11205> |
| Licence | MIT (<https://github.com/ggml-org/llama.cpp/blob/master/LICENSE>). The Windows CPU archive also contains `libomp.dll` with `LICENSE-LLVM-OpenMP` (Apache-2.0 WITH LLVM-exception). |
| Validated archive | `llama-b11205-bin-win-cpu-x64.zip`, 19,155,045 bytes, SHA-256 `2a8bf1ff808e8f6d2329f50f7005419a9930d9ecfbeae5047cfff475e87c2db0`, extracted-tree SHA-256 `cba79c95731ecfe372448629d5d39b26e52d74f92b50b5d8d5cc71d507fcfde8` (51 files) |
| Pinned, not validated | `win-vulkan-x64.zip` (`f3d08e7e…f563`), `win-cpu-arm64.zip` (`39adbe88…f455`), `ubuntu-x64.tar.gz` (`730ea8e5…6912`), `ubuntu-vulkan-x64.tar.gz` (`14bb34d7…d309`), `ubuntu-arm64.tar.gz` (`9106753b…437a`), `macos-arm64.tar.gz` (`97b06f59…d65e`), `macos-x64.tar.gz` (`1122a00c…4db1`); full hashes in `runtimes.toml`, taken from the GitHub release asset digests. `.tar.gz` archives cannot be installed by this build yet. |

The installed copy lives at `runtime/<platform>/llama/b11205-<variant>/` with an `aidex-runtime.json` manifest recording every file's SHA-256, the archive hash, licence, source, and URL. Every file is re-verified before each launch, and unexpected files are refused.

## `llama-server` interface AiDEX relies on

Launch flags (b11205 spelling): `--model`, `--host 127.0.0.1`, `--port`, `--ctx-size`, `--threads`, `--threads-batch`, `--batch-size`, `--ubatch-size`, `--n-gpu-layers`, `--parallel 1`, `--load-mode mmap|none`, `--jinja`, `--no-webui`, `--no-slots`, `--offline`. The API key comes from the `LLAMA_API_KEY` environment variable (never `--api-key`, which would be visible in the process list). HTTP: `GET /health` (503 while loading, 200 when ready, unauthenticated), `POST /v1/chat/completions` (SSE streaming, `response_format: json_schema` enforced by grammar, `tools` with `--jinja`, `timings` in non-streamed responses). Upgrading the pinned release means re-checking these flags (older releases used `--no-mmap` instead of `--load-mode`), adding new catalog entries with their digests, and re-running the real-runtime test.
