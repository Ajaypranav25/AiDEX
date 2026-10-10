//! `aidex offline on|off|status`: the persisted switch that refuses all remote
//! provider traffic (loopback/local models keep working).

use crate::{
    commands::daemon::connect,
    exit::{CliError, Exit},
    output::Output,
};
use aidexd::methods::models::{NETWORK_OFFLINE, NETWORK_STATUS, NetworkStatus};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct OfflineArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    #[command(subcommand)]
    pub command: OfflineCommand,
}

#[derive(Debug, Subcommand)]
pub enum OfflineCommand {
    /// Refuse every remote provider call (persisted on the drive).
    On,
    /// Allow remote providers again.
    Off,
    /// Show whether offline mode is on.
    Status,
}

impl OfflineCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::On => "aidex offline on",
            Self::Off => "aidex offline off",
            Self::Status => "aidex offline status",
        }
    }
}

pub fn run(args: OfflineArgs, output: Output) -> Result<Exit, CliError> {
    let mut client = connect(args.drive.as_deref())?;
    let status: NetworkStatus = match args.command {
        OfflineCommand::On => client.call_as(NETWORK_OFFLINE, json!({ "enabled": true }))?,
        OfflineCommand::Off => client.call_as(NETWORK_OFFLINE, json!({ "enabled": false }))?,
        OfflineCommand::Status => client.call_as(NETWORK_STATUS, Value::Null)?,
    };
    output.emit(&status, || {
        if status.offline {
            println!("Offline mode: on (remote providers are refused; local models still work)");
        } else {
            println!("Offline mode: off (remote providers allowed, subject to policy)");
        }
    })?;
    Ok(Exit::Success)
}
