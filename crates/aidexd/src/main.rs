use aidexd::server;

use aidex_host::HostInfo;
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

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
    let command = args.first().map(String::as_str).unwrap_or("help");
    match command {
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
        "run" => server::serve(&drive_arg(&args)?),
        "start" => start(&drive_arg(&args)?),
        "status" => {
            println!("{}", server::request(&drive_arg(&args)?, "status")?);
            Ok(())
        }
        "stop" => {
            println!("{}", server::request(&drive_arg(&args)?, "stop")?);
            Ok(())
        }
        "--help" | "-h" | "help" => {
            println!(
                "aidexd — local AiDEX daemon\n\nUsage: aidexd <start|stop|status|host> [--drive <PATH>]\n"
            );
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

fn start(root: &Path) -> Result<(), String> {
    server::recover_stale_endpoint(root)?;
    let executable =
        env::current_exe().map_err(|e| format!("cannot locate aidexd executable: {e}"))?;
    Command::new(executable)
        .arg("run")
        .arg("--drive")
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot launch daemon: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if server::request(root, "handshake").is_ok() {
            println!("ready");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("daemon did not become ready within 5 seconds".into())
}
