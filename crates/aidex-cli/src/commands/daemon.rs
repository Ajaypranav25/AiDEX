//! `aidex daemon start|status|stop`, plus [`connect`], the helper every
//! daemon-backed command uses to reach `aidexd`.

use crate::{
    exit::{CliError, Exit},
    output::{Output, display_path},
};
use aidex_drive::resolve_root;
use aidexd::{
    Client, ClientError, DaemonError,
    lifecycle::{self, STOP_TIMEOUT},
    methods::daemon::{DaemonState, DaemonStatus, STATUS},
};
use clap::Subcommand;
use serde_json::{Value, json};
use std::{
    env,
    path::{Path, PathBuf},
};

/// Client name reported in the daemon handshake.
pub const CLIENT_NAME: &str = "aidex-cli";

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the daemon on the selected drive.
    Start {
        /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
        #[arg(long, value_name = "DRIVE_PATH")]
        drive: Option<PathBuf>,
    },
    /// Report daemon readiness on the selected drive.
    Status {
        #[arg(long, value_name = "DRIVE_PATH")]
        drive: Option<PathBuf>,
    },
    /// Request graceful daemon shutdown and wait for it.
    Stop {
        #[arg(long, value_name = "DRIVE_PATH")]
        drive: Option<PathBuf>,
    },
}

fn start_hint(root: &Path) -> String {
    format!(
        "start it with `aidex daemon start --drive {}`",
        display_path(root)
    )
}

/// Name the drive in "not running" diagnostics; keep any extra detail.
fn not_running(root: &Path, mut error: CliError) -> CliError {
    let detail = if error.message.starts_with("daemon is not running") {
        String::new()
    } else {
        format!(": {}", error.message)
    };
    error.message = format!("daemon not running on {}{detail}", display_path(root));
    error
}

fn with_daemon_hint(error: CliError, root: &Path) -> CliError {
    match error.exit {
        Exit::DaemonUnavailable if error.hint.is_none() => error.with_hint(start_hint(root)),
        _ => error,
    }
}

/// Connect to the daemon serving `root`.
pub fn connect_root(root: &Path) -> Result<Client, CliError> {
    Client::connect(root, CLIENT_NAME).map_err(|error| {
        let unavailable = matches!(error, ClientError::Unavailable(_));
        let cli = CliError::from(error);
        let cli = if unavailable {
            not_running(root, cli)
        } else {
            cli
        };
        with_daemon_hint(cli, root)
    })
}

/// Resolve `--drive` (or `AIDEX_DRIVE_ROOT`) and connect, with actionable
/// diagnostics when the daemon is not running.
pub fn connect(drive: Option<&Path>) -> Result<Client, CliError> {
    connect_root(&resolve_root(drive)?)
}

pub fn run(command: DaemonCommand, output: Output) -> Result<Exit, CliError> {
    match command {
        DaemonCommand::Start { drive } => start(drive.as_deref(), output),
        DaemonCommand::Status { drive } => status(drive.as_deref(), output),
        DaemonCommand::Stop { drive } => stop(drive.as_deref(), output),
    }
}

fn print_status(status: &DaemonStatus) {
    let state = match status.state {
        DaemonState::Ready => "ready",
        DaemonState::Stopping => "stopping",
    };
    println!("Daemon: {state}");
    println!("PID: {}", status.pid);
    println!("Version: {}", status.version);
    println!("IPC contract version: {}", status.contract_version);
    println!("Drive root: {}", status.drive_root);
    println!("Uptime: {:.1} s", status.uptime_ms as f64 / 1000.0);
}

/// The `aidexd` executable next to this binary, else `aidexd` on PATH.
fn daemon_executable() -> PathBuf {
    let name = if cfg!(windows) {
        "aidexd.exe"
    } else {
        "aidexd"
    };
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(name)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("aidexd"))
}

fn start(drive: Option<&Path>, output: Output) -> Result<Exit, CliError> {
    let root = resolve_root(drive)?;
    let status = lifecycle::launch(&root, &daemon_executable()).map_err(|error| {
        let launch_failed = matches!(error, DaemonError::Launch(_));
        let cli = CliError::from(error);
        if launch_failed {
            cli.with_hint("build `aidexd` (cargo build -p aidexd) or place it next to `aidex`")
        } else {
            with_daemon_hint(cli, &root)
        }
    })?;
    output.emit(&status, || print_status(&status))?;
    Ok(Exit::Success)
}

fn status(drive: Option<&Path>, output: Output) -> Result<Exit, CliError> {
    let status: DaemonStatus = connect(drive)?.call_as(STATUS, Value::Null)?;
    output.emit(&status, || print_status(&status))?;
    Ok(Exit::Success)
}

fn stop(drive: Option<&Path>, output: Output) -> Result<Exit, CliError> {
    let root = resolve_root(drive)?;
    lifecycle::stop(&root, STOP_TIMEOUT).map_err(|error| {
        let unavailable = matches!(error, DaemonError::Client(ClientError::Unavailable(_)));
        let cli = CliError::from(error);
        if unavailable {
            not_running(&root, cli)
        } else {
            cli
        }
    })?;
    output.emit(&json!({ "state": "stopped" }), || {
        println!("Daemon: stopped")
    })?;
    Ok(Exit::Success)
}
