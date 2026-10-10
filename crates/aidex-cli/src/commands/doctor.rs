//! `aidex doctor`: host, active drive, daemon, and Oh My Pi diagnostics.
//!
//! Exits 0 only when the drive is healthy, Oh My Pi is available, and an installed
//! bundle (if any) verifies; the daemon check is informational. `integrity` reports
//! the installed bundle's version, signer key id, verification, and SBOM.

use crate::{
    commands::{
        agent::{AgentRuntime, oh_my_pi},
        daemon::CLIENT_NAME,
        drive::{DriveReport, print_status, print_unavailable},
        omp::install_hint,
        update::{local_integrity, print_integrity},
    },
    exit::{CliError, Exit},
    output::Output,
};
use aidex_bundle::VerifyScope;
use aidex_drive::{resolve_root, status as drive_status};
use aidex_host::HostInfo;
use aidex_updater::IntegrityReport;
use aidexd::{
    Client,
    methods::daemon::{DaemonStatus, STATUS},
};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
struct HostReport {
    os: &'static str,
    architecture: &'static str,
    working_directory: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct DriveCheck {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<DriveReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct DaemonCheck {
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    ok: bool,
    cli_version: &'static str,
    host: HostReport,
    drive: DriveCheck,
    daemon: DaemonCheck,
    oh_my_pi: AgentRuntime,
    /// The installed bundle for this host (critical files and signature, as the
    /// launcher checks them); `null` without an active drive.
    integrity: Option<IntegrityReport>,
}

pub fn run(output: Output) -> Result<Exit, CliError> {
    let host = HostInfo::current();
    let root = resolve_root(None);
    let (drive, daemon, human_drive): (DriveCheck, DaemonCheck, Box<dyn Fn()>) = match &root {
        Ok(root) => {
            let daemon = match Client::connect(root, CLIENT_NAME)
                .and_then(|mut client| client.call_as::<DaemonStatus>(STATUS, Value::Null))
            {
                Ok(status) => DaemonCheck {
                    running: true,
                    pid: Some(status.pid),
                    error: None,
                },
                Err(error) => DaemonCheck {
                    running: false,
                    pid: None,
                    error: Some(error.to_string()),
                },
            };
            match drive_status(root) {
                Ok(status) => {
                    let check = DriveCheck {
                        ok: status.writable,
                        status: Some(DriveReport::from(&status)),
                        error: None,
                    };
                    (check, daemon, Box::new(move || print_status(&status)))
                }
                Err(error) => {
                    let message = error.to_string();
                    let check = DriveCheck {
                        ok: false,
                        status: None,
                        error: Some(message.clone()),
                    };
                    let root = root.clone();
                    let print = move || {
                        print_unavailable(&root);
                        eprintln!("aidex doctor: {message}");
                    };
                    (check, daemon, Box::new(print))
                }
            }
        }
        Err(error) => {
            let message = error.to_string();
            let check = DriveCheck {
                ok: false,
                status: None,
                error: Some(message.clone()),
            };
            let daemon = DaemonCheck {
                running: false,
                pid: None,
                error: Some("no active drive".into()),
            };
            (
                check,
                daemon,
                Box::new(move || eprintln!("Drive: unavailable ({message})")),
            )
        }
    };
    let runtime = oh_my_pi();
    let integrity = root
        .as_ref()
        .ok()
        .map(|root| local_integrity(root, VerifyScope::Critical, false));
    let integrity_ok = integrity
        .as_ref()
        .is_none_or(|report| !report.installed || report.verified);
    let report = DoctorReport {
        ok: drive.ok && runtime.available && integrity_ok,
        cli_version: env!("CARGO_PKG_VERSION"),
        host: HostReport {
            os: host.os,
            architecture: host.architecture,
            working_directory: host.current_dir.clone(),
        },
        drive,
        daemon,
        oh_my_pi: runtime,
        integrity,
    };
    output.emit(&report, || {
        println!("AiDEX CLI {}", report.cli_version);
        println!("Host: {} ({})", report.host.os, report.host.architecture);
        if let Some(current_dir) = &report.host.working_directory {
            println!("Working directory: {}", current_dir.display());
        }
        human_drive();
        if let Some(integrity) = &report.integrity {
            print_integrity(integrity);
        }
        match (report.daemon.running, report.daemon.pid) {
            (true, Some(pid)) => println!("Daemon: running (pid {pid})"),
            _ => println!("Daemon: not running"),
        }
        match (&report.oh_my_pi.version, &report.oh_my_pi.error) {
            (Some(version), _) => println!("Oh My Pi: available ({version})"),
            (None, error) => {
                println!(
                    "Oh My Pi: unavailable ({})",
                    error.as_deref().unwrap_or("unknown error")
                );
                eprintln!("{}", install_hint());
            }
        }
    })?;
    Ok(if report.ok {
        Exit::Success
    } else {
        Exit::Failure
    })
}
