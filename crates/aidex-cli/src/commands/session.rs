//! `aidex session`: inspect durable sessions, their messages and checkpoints, adopt an
//! interrupted session for resumption, cancel, and branch. Every command goes through
//! the daemon's `session.*` methods.
//!
//! `resume` prints the resume plan and leaves the session paused on this host; it
//! says plainly that continuing the work needs the native agent harness, which is
//! not wired yet, instead of pretending to run anything.

mod render;

use crate::{
    commands::daemon::connect,
    exit::{CliError, Exit},
    output::Output,
};
use aidex_sessions::{MessagePage, Session, SessionStateKind};
use aidexd::methods::sessions::{
    BRANCH, CANCEL, CHECKPOINTS, CREATE, CheckpointList, DEFAULT_AGENT, LIST, MESSAGES, RESUME,
    ResumePlanView, SHOW, SessionList, SessionSummary,
};
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, Args)]
pub struct SessionArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    #[command(subcommand)]
    pub command: SessionCommand,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// Create a session record in a registered workspace (nothing is started).
    Create {
        /// Exact workspace id.
        #[arg(long, value_name = "ID")]
        workspace: String,
        /// Agent id recorded for the session.
        #[arg(long, value_name = "ID", default_value = DEFAULT_AGENT)]
        agent: String,
        /// Session title.
        #[arg(value_name = "TITLE", required = true, num_args = 1..)]
        title: Vec<String>,
    },
    /// List sessions, newest first.
    List {
        /// Only sessions of this workspace id.
        #[arg(long, value_name = "ID")]
        workspace: Option<String>,
        /// Only sessions in this state (created, running, waiting_approval, paused,
        /// completed, failed, cancelled, interrupted).
        #[arg(long, value_parser = parse_state)]
        state: Option<SessionStateKind>,
        /// 1-500 (default 50).
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Show a session with record counts, its latest checkpoint, and event gaps.
    Show { id: String },
    /// Print a session's messages after a sequence number.
    Messages {
        id: String,
        /// Print messages with a sequence number greater than this.
        #[arg(long, default_value_t = 0)]
        after: u64,
        /// 1-500.
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// List a session's checkpoints and whether they verified.
    Checkpoints { id: String },
    /// Print the resume plan and adopt an interrupted session on this host (left
    /// paused; continuing the work needs the native agent harness).
    Resume { id: String },
    /// Cancel a session (it accepts no further work).
    Cancel { id: String },
    /// Branch a new session from a checkpoint.
    Branch {
        id: String,
        /// Checkpoint id (see `aidex session checkpoints <ID>`).
        #[arg(long, value_name = "CHECKPOINT")]
        checkpoint: String,
    },
}

fn parse_state(value: &str) -> Result<SessionStateKind, String> {
    value
        .parse()
        .map_err(|error: aidex_sessions::SessionError| error.to_string())
}

impl SessionCommand {
    /// Prefix for human error messages.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Create { .. } => "aidex session create",
            Self::List { .. } => "aidex session list",
            Self::Show { .. } => "aidex session show",
            Self::Messages { .. } => "aidex session messages",
            Self::Checkpoints { .. } => "aidex session checkpoints",
            Self::Resume { .. } => "aidex session resume",
            Self::Cancel { .. } => "aidex session cancel",
            Self::Branch { .. } => "aidex session branch",
        }
    }
}

pub fn run(args: SessionArgs, output: Output) -> Result<Exit, CliError> {
    let drive = args.drive.as_deref();
    match args.command {
        SessionCommand::Create {
            workspace,
            agent,
            title,
        } => {
            let params =
                json!({"workspace_id": workspace, "agent_id": agent, "title": title.join(" ")});
            let session: Session = call(drive, CREATE, params)?;
            output.emit(&session, || {
                println!("Created session {} ({})", session.title, session.id);
                render::session_details(&session);
            })?;
        }
        SessionCommand::List {
            workspace,
            state,
            limit,
        } => {
            let mut params = json!({});
            if let Some(workspace) = workspace {
                params["workspace_id"] = json!(workspace);
            }
            if let Some(state) = state {
                params["state"] = json!(state);
            }
            if let Some(limit) = limit {
                params["limit"] = json!(limit);
            }
            let list: SessionList = call(drive, LIST, params)?;
            output.emit(&list, || render::session_table(&list.sessions))?;
        }
        SessionCommand::Show { id } => {
            let summary: SessionSummary = call(drive, SHOW, json!({ "id": id }))?;
            output.emit(&summary, || render::summary(&summary))?;
        }
        SessionCommand::Messages { id, after, limit } => {
            let page: MessagePage = call(
                drive,
                MESSAGES,
                json!({"id": id, "after": after, "limit": limit}),
            )?;
            output.emit(&page, || render::messages(&page))?;
        }
        SessionCommand::Checkpoints { id } => {
            let list: CheckpointList = call(drive, CHECKPOINTS, json!({ "id": id }))?;
            output.emit(&list, || render::checkpoints(&list))?;
        }
        SessionCommand::Resume { id } => {
            let plan: ResumePlanView = call(drive, RESUME, json!({ "id": id }))?;
            output.emit(&plan, || render::resume_plan(&plan))?;
        }
        SessionCommand::Cancel { id } => {
            let session: Session = call(drive, CANCEL, json!({ "id": id }))?;
            output.emit(&session, || {
                println!("Cancelled session {} ({})", session.title, session.id);
            })?;
        }
        SessionCommand::Branch { id, checkpoint } => {
            let session: Session = call(
                drive,
                BRANCH,
                json!({"id": id, "checkpoint_id": checkpoint}),
            )?;
            output.emit(&session, || {
                println!("Branched session {} ({})", session.title, session.id);
                render::session_details(&session);
            })?;
        }
    }
    Ok(Exit::Success)
}

fn call<T: serde::de::DeserializeOwned>(
    drive: Option<&Path>,
    method: &str,
    params: Value,
) -> Result<T, CliError> {
    Ok(connect(drive)?.call_as(method, params)?)
}

#[cfg(test)]
mod tests {
    use crate::cli::{Cli, Commands};
    use clap::Parser;

    use super::*;

    fn parse(args: &[&str]) -> SessionCommand {
        let cli =
            Cli::try_parse_from(std::iter::once("aidex").chain(args.iter().copied())).unwrap();
        match cli.command {
            Some(Commands::Session(args)) => args.command,
            other => panic!("expected a session command, got {other:?}"),
        }
    }

    #[test]
    fn session_subcommands_parse() {
        assert!(matches!(
            parse(&["session", "list", "--state", "interrupted", "--limit", "5"]),
            SessionCommand::List {
                state: Some(SessionStateKind::Interrupted),
                limit: Some(5),
                ..
            }
        ));
        assert!(matches!(
            parse(&["session", "branch", "ses_1", "--checkpoint", "chk_2"]),
            SessionCommand::Branch { .. }
        ));
        let SessionCommand::Create { title, agent, .. } = parse(&[
            "session",
            "create",
            "--workspace",
            "wsp_1",
            "fix",
            "the",
            "bug",
        ]) else {
            panic!("expected create");
        };
        assert_eq!(
            (title.join(" ").as_str(), agent.as_str()),
            ("fix the bug", DEFAULT_AGENT)
        );
        assert!(Cli::try_parse_from(["aidex", "session", "list", "--state", "sleeping"]).is_err());
        assert!(Cli::try_parse_from(["aidex", "session", "branch", "ses_1"]).is_err());
    }
}
