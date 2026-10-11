//! `aidex memory list|add|edit|remove [--workspace W|--user|--session S]`.

use super::{call, render};
use crate::{
    exit::{CliError, Exit},
    output::Output,
};
use aidex_context::{MemoryEntry, MemoryScopeKind};
use aidexd::methods::memory::{ADD, EDIT, LIST, MemoryList, REMOVE};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct MemoryArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    #[command(flatten)]
    pub scope: ScopeArgs,
    #[command(subcommand)]
    pub command: MemoryCommand,
}

/// Which memory to use; the default is the workspace scope.
#[derive(Debug, Args)]
pub struct ScopeArgs {
    /// Workspace id or name (default: the selected workspace).
    #[arg(long, value_name = "ID|NAME", global = true, conflicts_with_all = ["user", "session"])]
    pub workspace: Option<String>,
    /// Portable user memory (cross-project preferences).
    #[arg(long, global = true, conflicts_with = "session")]
    pub user: bool,
    /// Memory of one session.
    #[arg(long, value_name = "ID", global = true)]
    pub session: Option<String>,
}

impl ScopeArgs {
    fn params(&self) -> Value {
        let mut params = json!({});
        if self.user {
            params["scope"] = json!(MemoryScopeKind::User);
        } else if let Some(session) = &self.session {
            params["scope"] = json!(MemoryScopeKind::Session);
            params["session_id"] = json!(session);
        } else {
            params["scope"] = json!(MemoryScopeKind::Workspace);
            if let Some(workspace) = &self.workspace {
                params["workspace_id"] = json!(workspace);
            }
        }
        params
    }
}

#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// List memories in the scope.
    List,
    /// Add a memory (secret-like text is refused; use the vault for secrets).
    Add {
        /// Tag (repeatable): a-z, 0-9, `.`, `_`, `-`.
        #[arg(long = "tag", value_name = "TAG")]
        tags: Vec<String>,
        #[arg(value_name = "TEXT", required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// Change a memory's text or tags.
    Edit {
        id: String,
        /// New text.
        #[arg(long, value_name = "TEXT")]
        text: Option<String>,
        /// Replace the tags (repeatable).
        #[arg(long = "tag", value_name = "TAG", conflicts_with = "clear_tags")]
        tags: Vec<String>,
        /// Remove every tag.
        #[arg(long)]
        clear_tags: bool,
    },
    /// Remove a memory.
    Remove { id: String },
}

impl MemoryCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::List => "aidex memory list",
            Self::Add { .. } => "aidex memory add",
            Self::Edit { .. } => "aidex memory edit",
            Self::Remove { .. } => "aidex memory remove",
        }
    }
}

pub fn run(args: MemoryArgs, output: Output) -> Result<Exit, CliError> {
    let drive = args.drive.as_deref();
    let mut params = args.scope.params();
    match args.command {
        MemoryCommand::List => {
            let list: MemoryList = call(drive, LIST, params)?;
            output.emit(&list, || render::memories(&list))?;
        }
        MemoryCommand::Add { tags, text } => {
            params["text"] = json!(text.join(" "));
            if !tags.is_empty() {
                params["tags"] = json!(tags);
            }
            let entry: MemoryEntry = call(drive, ADD, params)?;
            output.emit(&entry, || {
                println!("Added memory {}", entry.id);
                render::memory(&entry);
            })?;
        }
        MemoryCommand::Edit {
            id,
            text,
            tags,
            clear_tags,
        } => {
            if text.is_none() && tags.is_empty() && !clear_tags {
                return Err(CliError::new(
                    Exit::Usage,
                    "nothing to change: pass --text, --tag, or --clear-tags",
                ));
            }
            params["id"] = json!(id);
            if let Some(text) = text {
                params["text"] = json!(text);
            }
            if clear_tags || !tags.is_empty() {
                params["tags"] = json!(tags);
            }
            let entry: MemoryEntry = call(drive, EDIT, params)?;
            output.emit(&entry, || {
                println!("Updated memory {}", entry.id);
                render::memory(&entry);
            })?;
        }
        MemoryCommand::Remove { id } => {
            params["id"] = json!(id);
            let entry: MemoryEntry = call(drive, REMOVE, params)?;
            output.emit(&entry, || println!("Removed memory {}", entry.id))?;
        }
    }
    Ok(Exit::Success)
}
