# GitHub Actions

`ci.yml` runs on every pull request and on pushes to `main`. The `rust` job checks the Rust workspace:

| Step | Runners |
| --- | --- |
| `cargo fmt --all -- --check` | `ubuntu-latest` |
| `cargo clippy --workspace --all-targets -- -D warnings` | `windows-latest`, `ubuntu-latest`, `macos-latest` |
| `cargo test --workspace --no-fail-fast` | `windows-latest`, `ubuntu-latest`, `macos-latest` |

The `apps` job checks the TypeScript packages and both Tauri apps:

| Step | Runners |
| --- | --- |
| `bun run --cwd packages/ui check`, `bun run --cwd packages/client check` | `windows-latest`, `ubuntu-latest` |
| `bun run test` (Vitest for `@aidex/client`, `@aidex/ui`, and `apps/desktop`; `@aidex/client` enforces 80% coverage) | `windows-latest`, `ubuntu-latest` |
| `bun run build` in `apps/desktop` and `apps/installer` (`next build` and `tsc --noEmit`) | `windows-latest`, `ubuntu-latest` |
| `cargo fmt`, `cargo clippy -D warnings`, and `cargo test` for both `src-tauri` crates | `ubuntu-latest` (fmt), `windows-latest`, `ubuntu-latest` |

The `e2e` job runs the Playwright smoke tests in `apps/desktop/e2e` on `ubuntu-latest`: Chromium opens the Next.js dev server outside Tauri, takes the sample-data path through the launch screens, visits every rail destination and tab, and walks back and forward. A failing run uploads the Playwright report and traces as the `playwright-report` artifact.

The toolchain is stable Rust with `rustfmt` and `clippy` added explicitly, because `rust-toolchain.toml` uses the minimal profile. `Swatinem/rust-cache` caches dependencies per runner. Bun is pinned to the `packageManager` version. Action major versions are pinned. Tests run with `--no-fail-fast` so one failing crate does not hide results from the others.

`package.yml` builds and smoke-tests portable bundles on pushes to `main`, on pull requests that touch `crates/bundle`, `crates/launcher`, `crates/packager`, `portable/packaging`, or the workflow, and on manual dispatch:

| Target | Runner | Triple |
| --- | --- | --- |
| `windows-x64` | `windows-latest` | `x86_64-pc-windows-msvc` |
| `linux-x64` | `ubuntu-latest` (with `musl-tools`) | `x86_64-unknown-linux-musl` |
| `macos-arm64` | `macos-latest` | `aarch64-apple-darwin` |

Each job runs `aidex-packager build`, then `aidex-packager --json smoke` against a fresh developer drive under `$RUNNER_TEMP` (launcher verification, `--version`, `doctor --json`, daemon start/status/stop, host-residue and drive-root checks), and uploads the archive, `SHA256SUMS`, and the smoke report as the `aidex-<target>` artifact. `linux-arm64` and `macos-x64` are not built.

CI does not bundle the Tauri apps, drive them inside a real webview, or exercise real removable drives; tests use temporary directories, and the smoke tests reach no daemon. A passing macOS or Linux run means the workspace compiles and its tests pass there, not that every platform-specific behaviour (for example removable-drive detection) has been validated on real hardware. Bundle artifacts are unsigned CI outputs, not releases; no release workflow is configured.
