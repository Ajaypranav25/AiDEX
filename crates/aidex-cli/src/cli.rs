//! Command-line definitions. Subcommand argument types live next to their
//! handlers in `commands/<domain>.rs`; add one variant here per domain.

use crate::commands::{
    agent::AgentCommand, daemon::DaemonCommand, events::EventsArgs, omp::SessionOptions,
    vault::VaultCommand, workspace::WorkspaceArgs,
};
use crate::commands::{model::ModelArgs, offline::OfflineArgs, provider::ProviderArgs};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "aidex", version, about = "Portable AI development environment")]
pub struct Cli {
    /// Print machine-readable JSON on stdout; errors become JSON on stderr.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Open an interactive agent session with approval prompts.
    Chat {
        #[command(flatten)]
        options: SessionOptions,
        /// Optional first message.
        #[arg(value_name = "PROMPT", num_args = 1.., trailing_var_arg = true)]
        prompt: Vec<String>,
    },
    /// Run a read-only, non-interactive agent prompt.
    Run {
        #[command(flatten)]
        options: SessionOptions,
        /// Prompt to send to the agent.
        #[arg(value_name = "PROMPT", required = true, num_args = 1.., trailing_var_arg = true)]
        prompt: Vec<String>,
    },
    /// Inspect configured agent runtimes.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Initialize AiDEX-owned directories on a removable drive.
    Init {
        /// Removable drive root (must already exist).
        #[arg(long, value_name = "DRIVE_PATH")]
        drive: PathBuf,
    },
    /// Report active drive health without changing it.
    Status {
        /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
        #[arg(long, value_name = "DRIVE_PATH")]
        drive: Option<PathBuf>,
    },
    /// Control or inspect the local daemon.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Stream durable workspace events, reconnecting from the last printed cursor.
    Events(EventsArgs),
    /// Check host information, drive health, and Oh My Pi availability.
    Doctor,
    /// Manage the drive's encrypted vault and its secrets.
    Vault {
        #[command(subcommand)]
        command: VaultCommand,
    },
    /// Manage host, drive, and ephemeral workspaces.
    #[command(alias = "project")]
    Workspace(WorkspaceArgs),
    /// Inspect models, choose the default, and explain routing.
    Model(ModelArgs),
    /// Configure model providers (credentials come from the vault).
    Provider(ProviderArgs),
    /// Refuse or allow remote provider traffic.
    Offline(OfflineArgs),
}

impl Commands {
    /// Prefix for human error messages, e.g. `aidex daemon status`.
    pub fn label(command: Option<&Self>) -> &'static str {
        match command {
            None | Some(Self::Chat { .. }) => "aidex",
            Some(Self::Run { .. }) => "aidex run",
            Some(Self::Agent { .. }) => "aidex agent",
            Some(Self::Init { .. }) => "aidex init",
            Some(Self::Status { .. }) => "aidex status",
            Some(Self::Daemon { command }) => match command {
                DaemonCommand::Start { .. } => "aidex daemon start",
                DaemonCommand::Status { .. } => "aidex daemon status",
                DaemonCommand::Stop { .. } => "aidex daemon stop",
            },
            Some(Self::Events(_)) => "aidex events",
            Some(Self::Doctor) => "aidex doctor",
            Some(Self::Vault { command }) => command.label(),
            Some(Self::Workspace(args)) => args.command.label(),
            Some(Self::Model(args)) => args.command.label(),
            Some(Self::Provider(args)) => args.command.label(),
            Some(Self::Offline(args)) => args.command.label(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, error::ErrorKind};

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("aidex").chain(args.iter().copied()))
    }

    #[test]
    fn command_definitions_are_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn json_is_a_global_flag() {
        for args in [
            &["--json", "daemon", "status"][..],
            &["daemon", "status", "--json"],
            &["daemon", "--json", "status", "--drive", "x"],
        ] {
            let cli = parse(args).unwrap();
            assert!(cli.json, "{args:?}");
            assert!(matches!(
                cli.command,
                Some(Commands::Daemon {
                    command: DaemonCommand::Status { .. }
                })
            ));
        }
        assert!(!parse(&["doctor"]).unwrap().json);
    }

    #[test]
    fn subcommands_parse_their_arguments() {
        let cli = parse(&["daemon", "start", "--drive", "/media/aidex"]).unwrap();
        let Some(Commands::Daemon {
            command: DaemonCommand::Start { drive },
        }) = cli.command
        else {
            panic!("expected daemon start");
        };
        assert_eq!(drive, Some(PathBuf::from("/media/aidex")));

        let cli = parse(&["events", "--workspace", "ws", "--after", "7", "--follow"]).unwrap();
        let Some(Commands::Events(events)) = cli.command else {
            panic!("expected events");
        };
        assert_eq!(
            (events.workspace.as_str(), events.after, events.follow),
            ("ws", 7, true)
        );

        let cli = parse(&["run", "--cwd", ".", "explain", "this"]).unwrap();
        let Some(Commands::Run { prompt, options }) = cli.command else {
            panic!("expected run");
        };
        assert_eq!(prompt, ["explain", "this"]);
        assert_eq!(options.cwd, Some(PathBuf::from(".")));
        assert!(parse(&[]).unwrap().command.is_none());
        assert!(matches!(
            parse(&["agent", "list"]).unwrap().command,
            Some(Commands::Agent {
                command: AgentCommand::List
            })
        ));
    }

    #[test]
    fn usage_errors_are_reported_by_clap() {
        for args in [
            &["init"][..],
            &["run"],
            &["events"],
            &["daemon", "restart"],
            &["--bogus"],
        ] {
            let error = parse(args).unwrap_err();
            assert!(error.use_stderr(), "{args:?}");
            assert_ne!(error.kind(), ErrorKind::DisplayHelp);
        }
        assert_eq!(
            parse(&["--help"]).unwrap_err().kind(),
            ErrorKind::DisplayHelp
        );
    }

    #[test]
    fn labels_name_the_failing_command() {
        let cli = parse(&["daemon", "stop"]).unwrap();
        assert_eq!(Commands::label(cli.command.as_ref()), "aidex daemon stop");
        assert_eq!(Commands::label(None), "aidex");
    }
}
