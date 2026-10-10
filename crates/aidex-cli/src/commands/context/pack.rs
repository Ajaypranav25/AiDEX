//! `aidex context pack "<task>" --budget N`: a debug view of the context pack the
//! native agent would receive for a task.

use super::{call, render};
use crate::{
    exit::{CliError, Exit},
    output::Output,
};
use aidex_context::ContextPack;
use aidexd::methods::context::PACK;
use clap::{Args, Subcommand};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ContextArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    #[command(subcommand)]
    pub command: ContextCommand,
}

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Assemble a budgeted context pack for a task and show what it contains.
    Pack(PackArgs),
}

impl ContextCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pack(_) => "aidex context pack",
        }
    }
}

#[derive(Debug, Args)]
pub struct PackArgs {
    /// Workspace id or name (default: the selected workspace).
    #[arg(long, value_name = "ID|NAME")]
    pub workspace: Option<String>,
    /// Token budget for all items (1-100000).
    #[arg(long, value_name = "TOKENS")]
    pub budget: usize,
    /// Include this session's memory.
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
    /// Indexed file to include first (repeatable).
    #[arg(long = "path", value_name = "PATH")]
    pub paths: Vec<String>,
    /// Leave out project rules and instruction files.
    #[arg(long)]
    pub no_rules: bool,
    /// Leave out memory.
    #[arg(long)]
    pub no_memory: bool,
    /// Leave out retrieved chunks.
    #[arg(long)]
    pub no_search: bool,
    /// Also print each item's delimited text.
    #[arg(long)]
    pub text: bool,
    /// The task description.
    #[arg(value_name = "TASK", required = true, num_args = 1..)]
    pub task: Vec<String>,
}

pub fn run(args: ContextArgs, output: Output) -> Result<Exit, CliError> {
    let ContextCommand::Pack(pack) = args.command;
    let mut params = json!({
        "task": pack.task.join(" "),
        "budget": pack.budget,
        "rules": !pack.no_rules,
        "memory": !pack.no_memory,
        "search": !pack.no_search,
    });
    if let Some(workspace) = &pack.workspace {
        params["workspace_id"] = json!(workspace);
    }
    if let Some(session) = &pack.session {
        params["session_id"] = json!(session);
    }
    if !pack.paths.is_empty() {
        params["paths"] = json!(pack.paths);
    }
    let result: ContextPack = call(args.drive.as_deref(), PACK, params)?;
    output.emit(&result, || render::pack(&result, pack.text))?;
    Ok(Exit::Success)
}
