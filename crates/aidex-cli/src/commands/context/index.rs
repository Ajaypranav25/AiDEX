//! `aidex index build|status|cancel`.

use super::{call, render, workspace_params};
use crate::{
    commands::daemon::connect,
    exit::{CliError, Exit},
    output::Output,
};
use aidex_context::{IndexJobStarted, IndexStatus, JobState};
use aidexd::methods::context::{CancelResult, INDEX, INDEX_CANCEL, STATUS};
use clap::{Args, Subcommand};
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

/// How often `build` polls the job while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// How often progress is printed while waiting.
const PRINT_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Args)]
pub struct IndexArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    /// Workspace id or name (default: the selected workspace).
    #[arg(long, value_name = "ID|NAME", global = true)]
    pub workspace: Option<String>,
    #[command(subcommand)]
    pub command: IndexCommand,
}

#[derive(Debug, Subcommand)]
pub enum IndexCommand {
    /// Build or refresh the workspace index (incremental; resumes a cancelled build).
    Build {
        /// Start the job and return instead of waiting for it.
        #[arg(long)]
        no_wait: bool,
    },
    /// Show index counters, state, repository metadata, and the latest job.
    Status,
    /// Cancel the running index job (committed batches are kept).
    Cancel,
}

impl IndexCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Build { .. } => "aidex index build",
            Self::Status => "aidex index status",
            Self::Cancel => "aidex index cancel",
        }
    }
}

pub fn run(args: IndexArgs, output: Output) -> Result<Exit, CliError> {
    let drive = args.drive.as_deref();
    let params = workspace_params(args.workspace.as_deref());
    match args.command {
        IndexCommand::Build { no_wait } => {
            let mut client = connect(drive)?;
            let started: IndexJobStarted = client.call_as(INDEX, params.clone())?;
            if no_wait {
                output.emit(&started, || {
                    let verb = if started.already_running {
                        "Already indexing"
                    } else {
                        "Started indexing"
                    };
                    println!(
                        "{verb} workspace {} (job {})",
                        started.workspace_id, started.job_id
                    );
                    println!("Follow with `aidex index status`; cancel with `aidex index cancel`.");
                })?;
                return Ok(Exit::Success);
            }
            let mut last_print = Instant::now();
            let status = loop {
                let status: IndexStatus = client.call_as(STATUS, params.clone())?;
                let job = status
                    .job
                    .as_ref()
                    .filter(|job| job.job_id == started.job_id);
                match job {
                    Some(job) if job.state == JobState::Running => {
                        if !output.is_json() && last_print.elapsed() >= PRINT_INTERVAL {
                            render::progress(&job.progress);
                            last_print = Instant::now();
                        }
                    }
                    _ => break status,
                }
                thread::sleep(POLL_INTERVAL);
            };
            output.emit(&status, || render::status(&status))?;
            Ok(match status.job.as_ref().map(|job| job.state) {
                Some(JobState::Completed) => Exit::Success,
                Some(JobState::Cancelled) => Exit::Cancelled,
                _ => Exit::Failure,
            })
        }
        IndexCommand::Status => {
            let status: IndexStatus = call(drive, STATUS, params)?;
            output.emit(&status, || render::status(&status))?;
            Ok(Exit::Success)
        }
        IndexCommand::Cancel => {
            let result: CancelResult = call(drive, INDEX_CANCEL, params)?;
            output.emit(&result, || {
                if result.cancelled {
                    println!("Cancelling the index job of {}", result.workspace_id);
                } else {
                    println!("No index job is running for {}", result.workspace_id);
                }
            })?;
            Ok(Exit::Success)
        }
    }
}
