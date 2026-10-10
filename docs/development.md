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
bun run dev:cli -- doctor
bun run dev:desktop
bun run dev:installer
cargo check --workspace
```

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
