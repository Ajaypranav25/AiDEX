# llama.cpp

AiDEX runs GGUF models with the upstream [llama.cpp](https://github.com/ggml-org/llama.cpp) `llama-server`, unmodified. Nothing from llama.cpp is vendored or redistributed by this repository. `aidex-inference` installs a runtime only from a local, flat distribution directory whose `runtime-manifest.json` lists every file with its SHA-256 and size (see `crates/inference/README.md`); the manifest is the integrity authority, so it must be produced from an archive whose digest matches the release.

## Pinned release

| Field | Value |
| --- | --- |
| Release | `b11205` — <https://github.com/ggml-org/llama.cpp/releases/tag/b11205> |
| Licence | MIT (<https://github.com/ggml-org/llama.cpp/blob/master/LICENSE>); keep the licence text with any redistributed copy. The Windows CPU archive also contains `libomp.dll` with `LICENSE-LLVM-OpenMP` (Apache-2.0 WITH LLVM-exception). |
| Validated archive | `llama-b11205-bin-win-cpu-x64.zip`, 19,155,045 bytes, SHA-256 `2a8bf1ff808e8f6d2329f50f7005419a9930d9ecfbeae5047cfff475e87c2db0`, 51 files in a flat layout (`llama-server.exe` is a 9 KiB launcher for `llama-server-impl.dll`; the `ggml-cpu-*.dll` variants are selected at run time). |

Other b11205 archives and their release digests (pinned for reference, **not validated** by AiDEX):

| Archive | SHA-256 |
| --- | --- |
| `llama-b11205-bin-win-vulkan-x64.zip` | `f3d08e7e342e2cb96ccddd1a208320c188d0a394844553a40a32c5e9903af563` |
| `llama-b11205-bin-win-cpu-arm64.zip` | `39adbe8846a1228f0b16e93ba726e8c05c5af46d990441b272f7eed64f8ff455` |
| `llama-b11205-bin-ubuntu-x64.tar.gz` | `730ea8e54b97735abb46e46747999249719251f29753fcda97b5e2f2f3ea6912` |
| `llama-b11205-bin-ubuntu-vulkan-x64.tar.gz` | `14bb34d7058693f7679117ec3544a732e0a72405c6a08d60c8961b79393dd309` |
| `llama-b11205-bin-ubuntu-arm64.tar.gz` | `9106753bff55e43c78ff595bf169a6bf01175f3d38f707866a1142553d4c437a` |
| `llama-b11205-bin-macos-arm64.tar.gz` | `97b06f59ad15e2b4b6044ba7338c4e3f40354c6dc2b9c5cda234e2bd6b9fd65e` |
| `llama-b11205-bin-macos-x64.tar.gz` | `1122a00cf6ce9e9ae687c1b71fef82e2f59bec2bea443f4746d94212b3124db1` |

The Linux and macOS archives are `.tar.gz` and were not inspected or run by AiDEX; the distribution format accepts only flat directories without links, so they need their own packaging step and validation.

## Producing a distribution

1. Download the archive from the release page and check its SHA-256 against the table above.
2. Extract it into an empty directory (the Windows archives are flat).
3. Write `runtime-manifest.json` with `version: 1`, `release: "b11205"`, the AiDEX platform name (`windows-x64`), `executable: "llama-server.exe"`, `gpu_supported: false` for CPU builds, and one `{path, sha256, size_bytes}` entry per file.
4. `aidex inference install-runtime <DIRECTORY>` verifies and copies it to `runtime/<platform>/llama-cpp/` on the drive.

`crates/inference/tests/real_llama.rs` performs steps 1–3 from the staged archive (and refuses an archive whose digest differs). The server is launched with `--model --host 127.0.0.1 --port --alias --ctx-size --threads --n-gpu-layers 0 --parallel 1 --jinja`; upgrading the release means re-checking those flags and the GGUF tensor layouts in `crates/inference/src/gguf.rs`, recording the new digests here, and re-running the real test.
