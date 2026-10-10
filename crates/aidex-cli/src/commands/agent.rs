//! `aidex agent list`: configured agent runtimes and their availability.

use crate::{
    commands::omp::omp_version,
    exit::{CliError, Exit},
    output::Output,
};
use clap::Subcommand;
use serde::Serialize;

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// List supported agent runtimes and their availability.
    List,
}

#[derive(Debug, Serialize)]
pub struct AgentRuntime {
    pub id: &'static str,
    pub name: &'static str,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentList {
    agents: Vec<AgentRuntime>,
}

pub fn oh_my_pi() -> AgentRuntime {
    let (version, error) = match omp_version() {
        Ok(version) => (Some(version), None),
        Err(error) => (None, Some(error)),
    };
    AgentRuntime {
        id: "omp",
        name: "Oh My Pi",
        available: version.is_some(),
        version,
        error,
    }
}

pub fn run(command: AgentCommand, output: Output) -> Result<Exit, CliError> {
    match command {
        AgentCommand::List => {
            let list = AgentList {
                agents: vec![oh_my_pi()],
            };
            output.emit(&list, || {
                for agent in &list.agents {
                    match &agent.version {
                        Some(version) => {
                            println!("{} ({}): available ({version})", agent.id, agent.name);
                        }
                        None => println!("{} ({}): unavailable", agent.id, agent.name),
                    }
                }
            })?;
            Ok(Exit::Success)
        }
    }
}
