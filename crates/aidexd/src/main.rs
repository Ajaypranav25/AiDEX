use aidex_host::HostInfo;
use std::{env, process::ExitCode};

fn main() -> ExitCode {
    match env::args().nth(1).as_deref() {
        Some("host") => {
            let host = HostInfo::current();
            println!("os: {}", host.os);
            println!("architecture: {}", host.architecture);
            println!(
                "executable: {}",
                host.executable
                    .as_deref()
                    .map_or_else(|| "unavailable".into(), |p| p.display().to_string())
            );
            println!(
                "working directory: {}",
                host.current_dir
                    .as_deref()
                    .map_or_else(|| "unavailable".into(), |p| p.display().to_string())
            );
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") | None => {
            println!(
                "aidexd — AiDEX daemon entry point (service runtime not initialized)\n\nUsage: aidexd host\n       aidexd --help\n\nThe host command reports process platform details; it does not start a service."
            );
            ExitCode::SUCCESS
        }
        Some(command) => {
            eprintln!("unknown command: {command}\nRun `aidexd --help` for usage.");
            ExitCode::from(2)
        }
    }
}
