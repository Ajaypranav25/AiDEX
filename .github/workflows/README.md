# GitHub Actions

`ci.yml` checks the Rust workspace on every pull request and on pushes to `main`:

| Step | Runners |
| --- | --- |
| `cargo fmt --all -- --check` | `ubuntu-latest` |
| `cargo clippy --workspace --all-targets -- -D warnings` | `windows-latest`, `ubuntu-latest`, `macos-latest` |
| `cargo test --workspace` | `windows-latest`, `ubuntu-latest`, `macos-latest` |

The toolchain is stable Rust with `rustfmt` and `clippy` added explicitly, because `rust-toolchain.toml` uses the minimal profile. `Swatinem/rust-cache` caches dependencies per runner. Action major versions are pinned.

CI does not build the Tauri desktop or installer apps, run Bun/TypeScript checks, or exercise real removable drives; tests use temporary directories. A passing macOS or Linux run means the workspace compiles and its tests pass there, not that every platform-specific behaviour (for example removable-drive detection) has been validated on real hardware. No release workflow is configured.
