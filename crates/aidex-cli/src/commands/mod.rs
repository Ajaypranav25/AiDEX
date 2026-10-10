//! Command handlers, one module per domain.
//!
//! A domain adds `commands/<domain>.rs` with its clap `Subcommand`/`Args` type
//! and `pub fn run(command, output: Output) -> Result<Exit, CliError>`, one
//! `pub mod` line below, one variant in `cli::Commands`, and one dispatch arm.
//! Daemon-backed commands obtain a client with [`daemon::connect`].

pub mod agent;
pub mod daemon;
pub mod doctor;
pub mod drive;
pub mod events;
pub mod omp;

use crate::{
    cli::Commands,
    exit::{CliError, Exit},
    output::Output,
};

pub fn dispatch(command: Option<Commands>, output: Output) -> Result<Exit, CliError> {
    match command {
        None => omp::chat(&omp::SessionOptions::default(), &[]),
        Some(Commands::Chat { options, prompt }) => omp::chat(&options, &prompt),
        Some(Commands::Run { options, prompt }) => omp::run(&options, &prompt),
        Some(Commands::Agent { command }) => agent::run(command, output),
        Some(Commands::Init { drive }) => drive::init(&drive, output),
        Some(Commands::Status { drive }) => drive::status(drive.as_deref(), output),
        Some(Commands::Daemon { command }) => daemon::run(command, output),
        Some(Commands::Events(args)) => events::run(args, output),
        Some(Commands::Doctor) => doctor::run(output),
    }
}
