# Host cleanup

Planned manifest-driven cleanup for AiDEX-managed host residue. Never claim removal of telemetry controlled by the host operating system.

Portable bundles run from the drive and extract nothing to the host (`bundle.json` declares `host_extraction: false`, and the launcher refuses bundles that do not), so the launcher leaves no residue to record; see [`portable/packaging/README.md`](../../portable/packaging/README.md).