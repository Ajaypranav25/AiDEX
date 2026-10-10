# Development workflow

## Tooling

- Bun manages the workspace and supplies the pinned Oh My Pi CLI/SDK package for the Rust `aidex` client.
- Cargo builds the Rust workspace, including the user-facing `aidex` binary and `aidexd`.
- Next.js 16 uses Turbopack for both Tauri frontends; static export is configured for Tauri.
- Tauri v2 prerequisites vary by host OS; consult the official Tauri setup guide before building on a new platform.

The frontend development servers bind to loopback on `3311` (desktop) and `3312` (installer). Keep subsequent local service ports in sequence starting at `3313`.

## Local commands

```sh
bun install
bun run dev:cli -- --help
bun run dev:cli -- init --drive /path/to/removable-drive
bun run dev:cli -- status --drive /path/to/removable-drive
bun run dev:cli -- doctor
bun run dev:desktop
bun run dev:installer
bun run test
bun run --cwd apps/desktop test:e2e
cargo check --workspace
```

`bun run test` runs the Vitest suites of `@aidex/client`, `@aidex/ui`, and `apps/desktop`; `test:e2e` runs the Playwright smoke tests against the desktop dev server in Chromium, outside Tauri (install the browser once with `bunx playwright install chromium` in `apps/desktop`).

The drive contract is the root-level schema-v2 JSON `aidex-drive.json` (see `portable/layouts/README.md`); `aidex init` and `aidex status` operate on that manifest and never use a second metadata format. `aidex init` migrates a schema-v1 manifest explicitly. Services locate drive directories through `aidex_drive::DrivePaths`; tests that need an initialized drive in a temporary directory use `aidex_drive::init_with` with `InitOptions { require_removable: false }`.

Daemon-backed behaviour is tested against an in-process `aidexd::Daemon` on a temporary directory (see `crates/aidexd/tests/common`); the daemon does not need an initialized drive. CI (`.github/workflows/ci.yml`) runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` on Windows, Linux, and macOS. It also type-checks, unit-tests, and builds both Tauri apps and their Rust backends on Windows and Linux, and runs the desktop smoke tests on Linux (see `.github/workflows/README.md`).

Portable bundles are built with `cargo run --locked -p aidex-packager -- build --target <target> --out dist` (release build into `target/packager/`, then `dist/aidex-<version>-<target>/`, its `.zip`/`.tar.gz`, and `dist/SHA256SUMS`). `aidex-packager install --bundle <archive> --drive <root>` installs one onto an initialized drive (`--init-dev-drive` initializes a non-removable developer folder first), and `aidex-packager smoke --archive <archive> --work <empty dir>` repeats the CI smoke test locally. Build each target on its own OS; Linux bundles use musl (`rustup target add x86_64-unknown-linux-musl` and `musl-tools`), or pass `--triple x86_64-unknown-linux-gnu` for a local glibc build. `.github/workflows/package.yml` builds and smoke-tests the Windows x64, Linux x64, and macOS arm64 bundles. See `portable/packaging/README.md`.

Run the narrowest relevant checks after changing a surface. Do not treat a successful frontend build as proof of drive provisioning, encryption, broker enforcement, or model inference; those behaviors require their own integration scenarios when implemented.

## Dependency boundaries

- Put portable domain behavior and the user-facing command surface in Rust crates; Tauri commands should remain thin adapters.
- The CLI delegates interactive work to Oh My Pi's supported `omp` executable and uses its restricted TypeScript SDK for headless read-only runs. Do not depend on Oh My Pi's private Rust crates; its repository documents them as implementation details behind its TypeScript package.
- Keep fetched third-party binaries, models, credentials, and generated build output out of source control.
- Add a workspace or service only when it has an owned contract and a real consumer.

## Contribution checklist

1. Read the affected domain documentation and preserve the security invariants.
2. Keep each change independently reviewable and use Conventional Commit subjects.
3. Run formatting, linting, type checking, tests, or a direct smoke scenario for the changed surface.
4. Update documentation when behavior or operational requirements change.
5. Review staged files for secrets, build artifacts, and unrelated changes before committing.
