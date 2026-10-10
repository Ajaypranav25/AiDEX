# aidex-packager

Developer and CI tool for portable bundles. It is never shipped inside a bundle.

```sh
cargo run --locked -p aidex-packager -- build --target windows-x64 --out dist
cargo run -p aidex-packager -- install --bundle dist/aidex-0.1.0-windows-x64.zip --drive H:\
cargo run -p aidex-packager -- --json smoke --archive dist/aidex-0.1.0-windows-x64.zip --work <empty dir>
```

- `build --target <t> [--out dist] [--triple <triple>] [--target-dir <dir>] [--manifest-path <Cargo.toml>] [--desktop <path>] [--installer <path>]`: `cargo build --release --locked` of `aidex`, `aidexd`, `aidex-sandbox-exec`, and `aidex-launcher` with the packaging flags (static CRT and `/Brepro` on Windows, remapped source paths, stripped symbols, `SOURCE_DATE_EPOCH`), license collection from `cargo metadata`, `bundle.json`, full verification, a deterministic `.zip`/`.tar.gz`, and a `SHA256SUMS` entry. Reports file counts, sizes, digests, and timings.
- `install --bundle <archive|dir> --drive <root> [--init-dev-drive]`: `aidex_bundle::install`. `--init-dev-drive` first initializes a non-removable developer folder (explicit opt-in only).
- `smoke --archive <file> --work <dir> [--timeout <s>]`: checks `SHA256SUMS`, installs into `<work>/drive`, runs the drive-root entry (`--launcher-verify`, `--version`, `doctor --json`, `daemon start`, `daemon status --json`, `daemon stop`), and fails on host residue in the watched per-user directories or unexpected names at the drive root.
- `--json` prints one JSON report on stdout; errors go to stderr (`{"error":{"message":...}}` with `--json`) with exit code 1.

Why a separate crate: the packager drives `cargo` and `git`, parses `cargo metadata`, and writes archives — dependencies and code that the launcher, which runs on every start, must not carry. The install logic it calls lives in `aidex-bundle`, shared with the updater.

See [`portable/packaging/README.md`](../../portable/packaging/README.md) for the build settings, layout, schemas, reproducibility notes, and the platform matrix.

## Limitations

- Builds the host's own targets only; cross-compilation is untested.
- Does not build or sign the Tauri apps; `--desktop`/`--installer` copy prebuilt artifacts and refuse symbolic links.
- License metadata is not an SBOM (CycloneDX, provenance, and signatures are #27).
- The smoke's host-residue check lists the top level of the watched directories only.
