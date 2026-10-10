use aidex_host::HostInfo;
use clap::{Args, Parser, Subcommand};
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const CHAT_TOOLS: &str = "read,grep,glob,find,write,edit,bash";

#[derive(Debug, Parser)]
#[command(name = "aidex", version, about = "Portable AI development environment")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Default, Args)]
struct SessionOptions {
    /// Project directory to open.
    #[arg(long, value_name = "DIR")]
    cwd: Option<PathBuf>,

    /// Model selector understood by Oh My Pi.
    #[arg(long, value_name = "MODEL")]
    model: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Commands {
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
        command: AgentCommands,
    },
    /// Check host information and Oh My Pi availability.
    Doctor,
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
    /// List supported agent runtimes and their availability.
    List,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Some(Commands::Chat { options, prompt }) => launch_omp(&options, &prompt),
        Some(Commands::Run { options, prompt }) => launch_sdk(&options, &prompt),
        Some(Commands::Agent {
            command: AgentCommands::List,
        }) => list_agents(),
        Some(Commands::Doctor) => doctor(),
        None => launch_omp(&SessionOptions::default(), &[]),
    }
}
fn launch_omp(options: &SessionOptions, prompt: &[String]) -> ExitCode {
    if let Some(cwd) = options.cwd.as_deref() {
        if let Err(error) = validate_cwd(cwd) {
            eprintln!("aidex: {error}");
            return ExitCode::from(2);
        }
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
        Ok(status) => exit_code(status.code()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            print_runtime_install_hint();
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("aidex: could not start Oh My Pi: {error}");
            ExitCode::FAILURE
        }
    }
}

fn launch_sdk(options: &SessionOptions, prompt: &[String]) -> ExitCode {
    if let Some(cwd) = options.cwd.as_deref() {
        if let Err(error) = validate_cwd(cwd) {
            eprintln!("aidex: {error}");
            return ExitCode::from(2);
        }
    }

    let runner = env::var_os("AIDEX_RUNNER")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/cli/src/run.ts"));
    if !runner.is_file() {
        eprintln!(
            "aidex: Oh My Pi SDK runner not found at `{}`; set AIDEX_RUNNER to its path",
            runner.display()
        );
        return ExitCode::FAILURE;
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
        Ok(status) => exit_code(status.code()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("aidex: Bun is required for the read-only Oh My Pi SDK runner.");
            eprintln!("Install Bun and run `bun install` from the repository root.");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("aidex: could not start Oh My Pi SDK runner: {error}");
            ExitCode::FAILURE
        }
    }
}

fn list_agents() -> ExitCode {
    match omp_version() {
        Ok(version) => println!("omp (Oh My Pi): available ({version})"),
        Err(_) => println!("omp (Oh My Pi): unavailable"),
    }
    ExitCode::SUCCESS
}

fn doctor() -> ExitCode {
    let host = HostInfo::current();
    println!("AiDEX CLI {}", env!("CARGO_PKG_VERSION"));
    println!("Host: {} ({})", host.os, host.architecture);
    if let Some(current_dir) = host.current_dir {
        println!("Working directory: {}", current_dir.display());
    }

    match omp_version() {
        Ok(version) => {
            println!("Oh My Pi: available ({version})");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("Oh My Pi: unavailable ({error})");
            print_runtime_install_hint();
            ExitCode::FAILURE
        }
    }
}

fn omp_version() -> Result<String, String> {
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

fn validate_cwd(cwd: &Path) -> Result<(), String> {
    let metadata = cwd
        .metadata()
        .map_err(|error| format!("cannot access `{}`: {error}", cwd.display()))?;
    if !metadata.is_dir() {
        return Err(format!("`{}` is not a directory", cwd.display()));
    }
    Ok(())
}

fn print_runtime_install_hint() {
    eprintln!("aidex: Oh My Pi is not available on PATH.");
    eprintln!("From the repository root, run `bun install`, then use `bun run dev:cli`.");
    eprintln!("For a global installation, run `bun install -g @oh-my-pi/pi-coding-agent@18.8.7`.");
    eprintln!("Set AIDEX_OMP to the executable path to use a non-default installation.");
}

fn exit_code(code: Option<i32>) -> ExitCode {
    ExitCode::from(code.unwrap_or(1).clamp(0, u8::MAX as i32) as u8)
}
