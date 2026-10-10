//! Test stand-in for the `aidex` CLI: prints its arguments and launcher-provided
//! environment as one JSON line, then exits with the code given as `exit <code>`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = |name: &str| std::env::var(name).ok();
    let first_path_entry = std::env::var_os("PATH")
        .and_then(|path| std::env::split_paths(&path).next())
        .map(|entry| entry.display().to_string());
    let report = serde_json::json!({
        "args": args,
        "drive_root": env("AIDEX_DRIVE_ROOT"),
        "bundle_dir": env("AIDEX_BUNDLE_DIR"),
        "bundle_version": env("AIDEX_BUNDLE_VERSION"),
        "bundle_target": env("AIDEX_BUNDLE_TARGET"),
        "launcher_version": env("AIDEX_LAUNCHER_VERSION"),
        "path_first": first_path_entry,
        "current_dir": std::env::current_dir().ok().map(|dir| dir.display().to_string()),
    });
    println!("{report}");
    let code = match args.as_slice() {
        [command, code, ..] if command == "exit" => code.parse().unwrap_or(1),
        _ => 0,
    };
    std::process::exit(code);
}
