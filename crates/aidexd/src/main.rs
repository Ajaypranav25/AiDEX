use aidex_host::HostInfo;
use aidexd::{
    Client,
    lifecycle::{self, STOP_TIMEOUT},
    methods::daemon::{DaemonStatus, STATUS},
};
use serde_json::Value;
use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

const USAGE: &str = "aidexd — local AiDEX daemon

Usage: aidexd <command> [--drive <PATH>]
       aidexd --version

Commands:
  start   launch a detached daemon for the drive and wait until it is ready
  run     serve in the foreground (used by `start`)
  status  print daemon status as JSON
  stop    request graceful shutdown and wait for it
  host    print host facts
";

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("aidexd: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.first().map(String::as_str).unwrap_or("help") {
        "host" => {
            let host = HostInfo::current();
            println!("os: {}", host.os);
            println!("architecture: {}", host.architecture);
            println!(
                "executable: {}",
                host.executable
                    .as_deref()
                    .map_or_else(|| "unavailable".into(), |p| p.display().to_string())
            );
            Ok(())
        }
        "run" => lifecycle::serve(&drive_arg(&args)?)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "start" => {
            let executable =
                env::current_exe().map_err(|e| format!("cannot locate aidexd executable: {e}"))?;
            lifecycle::launch(&drive_arg(&args)?, &executable).map_err(|e| e.to_string())?;
            println!("ready");
            Ok(())
        }
        "status" => {
            let status: DaemonStatus = Client::connect(&drive_arg(&args)?, "aidexd")
                .and_then(|mut client| client.call_as(STATUS, Value::Null))
                .map_err(|e| e.to_string())?;
            let json = serde_json::to_string_pretty(&status).map_err(|e| e.to_string())?;
            println!("{json}");
            Ok(())
        }
        "stop" => {
            lifecycle::stop(&drive_arg(&args)?, STOP_TIMEOUT).map_err(|e| e.to_string())?;
            println!("stopped");
            Ok(())
        }
        "--version" | "-V" | "version" => {
            println!("aidexd {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command {other}; run `aidexd --help`")),
    }
}

fn drive_arg(args: &[String]) -> Result<PathBuf, String> {
    let explicit = args
        .windows(2)
        .find(|pair| pair[0] == "--drive")
        .map(|pair| Path::new(&pair[1]));
    aidex_drive::resolve_root(explicit).map_err(|e| e.to_string())
}
