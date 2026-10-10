//! Interim agent delegation to Oh My Pi (`aidex chat`, `aidex run`).
//!
//! Kept as-is until the native AiDEX agent replaces it; this is not an AiDEX
//! sandbox or policy boundary.

use crate::exit::{CliError, Exit};
use clap::Args;
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

const CHAT_TOOLS: &str = "read,grep,glob,find,write,edit,bash";
/// Conventional exit status of a process interrupted by Ctrl-C.
const INTERRUPTED: i32 = 130;

#[derive(Debug, Default, Args)]
pub struct SessionOptions {
    /// Project directory to open.
    #[arg(long, value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Model selector understood by Oh My Pi.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
}

pub fn install_hint() -> String {
    [
        "Oh My Pi is not available on PATH.",
        "From the repository root, run `bun install`, then use `bun run dev:cli`.",
        "For a global installation, run `bun install -g @oh-my-pi/pi-coding-agent@18.8.7`.",
        "Set AIDEX_OMP to the executable path to use a non-default installation.",
    ]
    .join("\n")
}

/// Map a delegated runtime's status onto the stable exit table.
fn delegated(status: ExitStatus) -> Exit {
    match status.code() {
        Some(0) => Exit::Success,
        Some(INTERRUPTED) | None => Exit::Cancelled,
        Some(_) => Exit::Failure,
    }
}

fn validate_cwd(cwd: &Path) -> Result<(), CliError> {
    let metadata = cwd.metadata().map_err(|error| {
        CliError::new(
            Exit::Usage,
            format!("cannot access `{}`: {error}", cwd.display()),
        )
    })?;
    if !metadata.is_dir() {
        return Err(CliError::new(
            Exit::Usage,
            format!("`{}` is not a directory", cwd.display()),
        ));
    }
    Ok(())
}

/// `aidex` / `aidex chat`: Oh My Pi's interactive TUI with approval prompts.
pub fn chat(options: &SessionOptions, prompt: &[String]) -> Result<Exit, CliError> {
    if let Some(cwd) = options.cwd.as_deref() {
        validate_cwd(cwd)?;
    }
    let mut command = omp_command();
    command.args([
        "--no-session",
        "--no-title",
        "--no-extensions",
        "--no-mcp",
        "--no-lsp",
        "--no-skills",
        "--approval-mode",
        "always-ask",
        "--tools",
        CHAT_TOOLS,
    ]);
    if let Some(cwd) = options.cwd.as_deref() {
        command.arg("--cwd").arg(cwd);
    }
    if let Some(model) = options.model.as_deref() {
        command.arg("--model").arg(model);
    }
    if !prompt.is_empty() {
        command.arg("--").arg(prompt.join(" "));
    }
    match command.status() {
        Ok(status) => Ok(delegated(status)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(CliError::new(Exit::Failure, "could not start Oh My Pi").with_hint(install_hint()))
        }
        Err(error) => Err(CliError::new(
            Exit::Failure,
            format!("could not start Oh My Pi: {error}"),
        )),
    }
}

/// `aidex run`: headless, read-only Oh My Pi SDK session.
pub fn run(options: &SessionOptions, prompt: &[String]) -> Result<Exit, CliError> {
    if let Some(cwd) = options.cwd.as_deref() {
        validate_cwd(cwd)?;
    }
    let runner = env::var_os("AIDEX_RUNNER")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/cli/src/run.ts"));
    if !runner.is_file() {
        return Err(CliError::new(
            Exit::Failure,
            format!(
                "Oh My Pi SDK runner not found at `{}`; set AIDEX_RUNNER to its path",
                runner.display()
            ),
        ));
    }
    let mut command = match env::var_os("AIDEX_BUN") {
        Some(path) => Command::new(path),
        None => Command::new("bun"),
    };
    command.arg("run").arg(runner).arg("--");
    if let Some(cwd) = options.cwd.as_deref() {
        command.arg("--cwd").arg(cwd);
    }
    if let Some(model) = options.model.as_deref() {
        command.arg("--model").arg(model);
    }
    command.arg("--").arg(prompt.join(" "));
    match command.status() {
        Ok(status) => Ok(delegated(status)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(CliError::new(
            Exit::Failure,
            "Bun is required for the read-only Oh My Pi SDK runner",
        )
        .with_hint("Install Bun and run `bun install` from the repository root.")),
        Err(error) => Err(CliError::new(
            Exit::Failure,
            format!("could not start Oh My Pi SDK runner: {error}"),
        )),
    }
}

pub fn omp_version() -> Result<String, String> {
    let output = omp_command()
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return Ok(if version.is_empty() {
            "version unavailable".to_owned()
        } else {
            version
        });
    }
    let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(if error.is_empty() {
        format!("exited with status {:?}", output.status)
    } else {
        error
    })
}

fn omp_command() -> Command {
    match env::var_os("AIDEX_OMP") {
        Some(path) => Command::new(path),
        None => Command::new("omp"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_working_directory_is_a_usage_error() {
        let missing = std::env::temp_dir().join("aidex-cli-missing-cwd-for-test");
        let options = SessionOptions {
            cwd: Some(missing),
            model: None,
        };
        assert_eq!(chat(&options, &[]).unwrap_err().exit, Exit::Usage);
        assert_eq!(run(&options, &["x".into()]).unwrap_err().exit, Exit::Usage);
    }
}
