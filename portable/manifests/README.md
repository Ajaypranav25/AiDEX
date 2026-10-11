# Drive manifests

Bundle integrity metadata is produced today by `aidex-packager`: each bundle carries a closed-schema `bundle.json` (files with SHA-256 digests, roles, licenses, state paths) and each build directory gets a `sha256sum`-compatible `SHA256SUMS`; see [`portable/packaging/README.md`](../packaging/README.md). The drive's `manifests/` directory is reserved for copies of checksums, SBOMs, and signatures, which the updater (#27) adds. No signed drive image or SBOM is generated yet.
