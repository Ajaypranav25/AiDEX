mod cli;
mod commands;
mod exit;
mod output;

use clap::Parser;
use cli::{Cli, Commands};
use exit::Exit;
use output::Output;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() {
                Exit::Usage
            } else {
                Exit::Success
            }
            .into();
        }
    };
    let output = Output::new(cli.json);
    let label = Commands::label(cli.command.as_ref());
    match commands::dispatch(cli.command, output) {
        Ok(exit) => exit.into(),
        Err(error) => {
            output.error(label, &error);
            error.exit.into()
        }
    }
}
