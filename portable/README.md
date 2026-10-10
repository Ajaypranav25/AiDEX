# Portable distributions

Portable layouts and packaging for AiDEX drives.

- [`layouts/`](layouts/README.md): the drive contract (`aidex-drive.json`, schema v2) and the directories it declares.
- [`packaging/`](packaging/README.md): self-contained CLI bundles, the drive-root launcher, the `bundle.json` and `app/current.json` formats, and the platform support matrix. Bundles are built, installed, and smoke-tested with `aidex-packager`; there are no signed release artifacts yet.
- `bootstrap/`, `manifests/`, `model-packs/`, `toolchains/`: reserved; see each README.

No embedded model payload or third-party toolchain is provided.
