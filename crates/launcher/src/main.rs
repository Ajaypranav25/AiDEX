//! `AiDEX.exe` / `aidex`: the portable drive-root launcher. It locates the drive from
//! its own path, verifies the drive manifest and the active bundle's critical binaries,
//! and only then runs the bundle's `aidex` CLI with the caller's arguments.

mod error;
mod info;
mod launch;
mod locate;

use std::process::ExitCode;

use launch::Mode;

fn main() -> ExitCode {
    let result = Mode::parse(std::env::args_os().skip(1).collect()).and_then(|mode| match mode {
        Mode::Run(args) => launch::run(args),
        Mode::Info(scope) => info::print(scope).map(|()| 0),
    });
    match result {
        // Windows exit codes are 32-bit; pass the CLI's code through unchanged.
        Ok(code) => std::process::exit(code),
        Err(error) => {
            error.report();
            ExitCode::from(u8::try_from(error.code).unwrap_or(1))
        }
    }
}
