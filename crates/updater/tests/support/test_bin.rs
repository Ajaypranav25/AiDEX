//! Test stand-in for the bundled `aidex` and `aidexd` binaries, used to exercise the
//! real process self-test. It reads the `bundle.json` one directory above its own
//! `bin/` and answers `--version` and `doctor --json` with that version. A version
//! containing `selftest-fail` makes `--version` exit 3; `selftest-hang` makes it
//! sleep far past any self-test deadline.

use std::{path::PathBuf, process::ExitCode, thread, time::Duration};

fn bundle_version() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let manifest: PathBuf = exe.parent()?.parent()?.join("bundle.json");
    let text = std::fs::read_to_string(manifest).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value["version"].as_str().map(str::to_owned)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let version = bundle_version().unwrap_or_else(|| "unknown".into());
    let name = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("--version") => {
            if version.contains("selftest-hang") {
                thread::sleep(Duration::from_secs(600));
            }
            if version.contains("selftest-fail") {
                eprintln!("{name}: simulated self-test failure");
                return ExitCode::from(3);
            }
            println!("{name} {version}");
            ExitCode::SUCCESS
        }
        Some("doctor") => {
            let drive = std::env::var("AIDEX_DRIVE_ROOT").unwrap_or_default();
            println!(
                "{}",
                serde_json::json!({ "ok": false, "cli_version": version, "drive_root": drive })
            );
            ExitCode::from(1)
        }
        _ => ExitCode::from(2),
    }
}
