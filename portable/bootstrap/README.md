# Portable bootstrap

The drive-root launcher is implemented in [`crates/launcher`](../../crates/launcher/README.md) (`AiDEX.exe` on Windows, `./aidex` on Linux and macOS) and specified in [`portable/packaging/README.md`](../packaging/README.md#launcher-behaviour). It verifies the installed bundle before running the `aidex` CLI from the drive. This directory stays reserved for future bootstrap assets (for example a signed macOS `AiDEX.app` entry); the app shells do not launch a daemon or claim drive readiness.
