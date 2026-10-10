//! `aidex rules show`: the workspace's `.aidex/rules.toml` and instruction files.

use super::{call, render, workspace_params};
use crate::{
    exit::{CliError, Exit},
    output::Output,
};
use aidexd::methods::context::{RULES_SHOW, RulesResult};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct RulesArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    /// Workspace id or name (default: the selected workspace).
    #[arg(long, value_name = "ID|NAME", global = true)]
    pub workspace: Option<String>,
    #[command(subcommand)]
    pub command: RulesCommand,
}

#[derive(Debug, Subcommand)]
pub enum RulesCommand {
    /// Show project commands, guardrails (tighten-only), and instruction files.
    Show,
}

impl RulesCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Show => "aidex rules show",
        }
    }
}

pub fn run(args: RulesArgs, output: Output) -> Result<Exit, CliError> {
    let RulesCommand::Show = args.command;
    let result: RulesResult = call(
        args.drive.as_deref(),
        RULES_SHOW,
        workspace_params(args.workspace.as_deref()),
    )?;
    output.emit(&result, || render::rules(&result))?;
    Ok(Exit::Success)
}
