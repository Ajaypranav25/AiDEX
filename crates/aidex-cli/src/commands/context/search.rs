//! `aidex search "<query>" [--symbol|--file]`: offline search of the workspace index.

use super::{call, render};
use crate::{
    exit::{CliError, Exit},
    output::Output,
};
use aidex_context::SearchMode;
use aidexd::methods::context::{SEARCH, SearchResult};
use clap::Args;
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH")]
    pub drive: Option<PathBuf>,
    /// Workspace id or name (default: the selected workspace).
    #[arg(long, value_name = "ID|NAME")]
    pub workspace: Option<String>,
    /// Match definition names (functions, types, constants).
    #[arg(long, conflicts_with = "file")]
    pub symbol: bool,
    /// Match file paths.
    #[arg(long)]
    pub file: bool,
    /// Only results under this workspace-relative path.
    #[arg(long, value_name = "PATH")]
    pub path: Option<String>,
    /// 1-200.
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    /// Words to search for (full-text search requires every word).
    #[arg(value_name = "QUERY", required = true, num_args = 1..)]
    pub query: Vec<String>,
}

impl SearchArgs {
    pub fn label(&self) -> &'static str {
        "aidex search"
    }

    fn mode(&self) -> SearchMode {
        if self.symbol {
            SearchMode::Symbol
        } else if self.file {
            SearchMode::Filename
        } else {
            SearchMode::Lexical
        }
    }
}

pub fn run(args: SearchArgs, output: Output) -> Result<Exit, CliError> {
    let mut params = json!({
        "query": args.query.join(" "),
        "mode": args.mode(),
        "limit": args.limit,
    });
    if let Some(workspace) = &args.workspace {
        params["workspace_id"] = json!(workspace);
    }
    if let Some(path) = &args.path {
        params["path_prefix"] = json!(path);
    }
    let result: SearchResult = call(args.drive.as_deref(), SEARCH, params)?;
    output.emit(&result, || render::search(&result))?;
    Ok(Exit::Success)
}
