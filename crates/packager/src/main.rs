//! `aidex-packager`: build, sign, verify, install, and smoke-test portable AiDEX bundles.
//!
//! A developer/CI tool, never shipped inside a bundle. It drives `cargo` and `git`,
//! collects license metadata from `cargo metadata`, and uses `aidex-bundle` for the
//! manifest, archive, and install logic shared with the launcher and updater.

mod build;
mod cli;
mod error;
mod install;
mod licenses;
mod output;
mod sbom;
mod signing;
mod smoke;
mod workspace;

use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Build(args) => {
            build::run(args).and_then(|report| output::build(&report, cli.json))
        }
        Command::Install(args) => {
            install::run(args).and_then(|report| output::install(&report, cli.json))
        }
        Command::Smoke(args) => {
            smoke::run(args).and_then(|report| output::smoke(&report, cli.json))
        }
        Command::Keygen(args) => {
            signing::keygen(args).and_then(|report| output::keygen(&report, cli.json))
        }
        Command::Sign(args) => {
            signing::sign(args).and_then(|report| output::sign(&report, cli.json))
        }
        Command::Verify(args) => {
            signing::verify(args).and_then(|report| output::verify(&report, cli.json))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if cli.json {
                eprintln!(
                    "{}",
                    serde_json::json!({ "error": { "message": error.message } })
                );
            } else {
                eprintln!("aidex-packager: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
