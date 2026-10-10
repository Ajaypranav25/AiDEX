# GitHub configuration

`workflows/ci.yml` runs Rust formatting, Clippy (warnings denied), and the workspace tests on Windows, Linux, and macOS; see `workflows/README.md` for exactly what it does and does not check. Add further workflows only when their commands, supported platforms, and toolchain versions are established; avoid workflows that claim checks they do not run.
