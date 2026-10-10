use aidex_drive::{init as init_drive, resolve_root, status as drive_status};
use aidex_events::{Event, MAX_BATCH};
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
        command: DaemonCommands,
    },
    /// Stream durable workspace events, reconnecting from the last printed cursor.
    Events {
        /// Workspace whose events to read.
        #[arg(long)]
        workspace: String,
        /// Last successfully emitted event sequence.
        #[arg(long, default_value_t = 0)]
        after: u64,
        /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
        #[arg(long)]
        drive: Option<PathBuf>,
        /// Continue polling and reconnect after daemon outages.
        #[arg(long)]
        follow: bool,
    },
    /// Check host information, drive health, and Oh My Pi availability.
    Doctor,
}

fn drive_init(path: &Path) -> ExitCode {
    match init_drive(path) {
        Ok(status) => {
            print_drive_status(
                &status.root,
                status.schema_version,
                status.writable,
                status.free_bytes,
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("aidex init: {error}");
            ExitCode::FAILURE
        }
    }
}

fn drive_health(path: Option<&Path>) -> ExitCode {
    let root = match resolve_root(path) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("aidex status: {error}");
            return ExitCode::FAILURE;
        }
    };
    match drive_status(&root) {
        Ok(status) => {
            print_drive_status(
                &status.root,
                status.schema_version,
                status.writable,
                status.free_bytes,
            );
            if status.writable {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            print_drive_error(&root, &error);
            ExitCode::FAILURE
        }
    }
}

fn print_drive_status(root: &Path, schema: Option<u32>, writable: bool, free: Option<u64>) {
    println!("Drive root: {}", root.display());
    println!(
        "Schema version: {}",
        schema.map_or_else(|| "unavailable".into(), |version| version.to_string())
    );
    println!("Writable: {}", if writable { "yes" } else { "no" });
    println!(
        "Free space: {}",
        free.map_or_else(|| "unavailable".into(), |bytes| format!("{bytes} bytes"))
    );
}

fn print_drive_error(root: &Path, error: &dyn std::fmt::Display) {
    println!("Drive root: {}", root.display());
    println!("Schema version: unavailable");
    println!("Writable: unknown");
    println!("Free space: unavailable");
    eprintln!("aidex status: {error}");
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
    /// List supported agent runtimes and their availability.
    List,
}

#[derive(Debug, Subcommand)]
enum DaemonCommands {
    /// Start the daemon on the selected drive.
    Start {
        #[arg(long)]
        drive: Option<PathBuf>,
    },
    /// Report daemon readiness on the selected drive.
    Status {
        #[arg(long)]
        drive: Option<PathBuf>,
    },
    /// Request graceful daemon shutdown.
    Stop {
        #[arg(long)]
        drive: Option<PathBuf>,
    },
}

fn daemon_command(command: DaemonCommands) -> ExitCode {
    let (operation, drive) = match command {
        DaemonCommands::Start { drive } => ("start", drive),
        DaemonCommands::Status { drive } => ("status", drive),
        DaemonCommands::Stop { drive } => ("stop", drive),
    };
    let root = match resolve_root(drive.as_deref()) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("aidex daemon {operation}: {error}");
            return ExitCode::FAILURE;
        }
    };
    if operation == "start" {
        let sibling = env::current_exe()
            .ok()
            .and_then(|path| {
                path.parent().map(|parent| {
                    parent.join(if cfg!(windows) {
                        "aidexd.exe"
                    } else {
                        "aidexd"
                    })
                })
            })
            .filter(|path| path.is_file());
        let mut launch = sibling.map_or_else(|| Command::new("aidexd"), Command::new);
        return match launch.arg("start").arg("--drive").arg(&root).status() {
            Ok(status) if status.success() => ExitCode::SUCCESS,
            Ok(status) => exit_code(status.code()),
            Err(error) => {
                eprintln!("aidex daemon start: cannot launch aidexd: {error}");
                ExitCode::FAILURE
            }
        };
    }
    match aidexd::server::request(&root, operation) {
        Ok(state) => {
            println!("Daemon: {state}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("aidex daemon {operation}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn events_command(
    workspace: &str,
    mut cursor: u64,
    drive: Option<&Path>,
    follow: bool,
) -> ExitCode {
    let root = match resolve_root(drive) {
        Ok(root) => root,
        Err(error) => {
            eprintln!("aidex events: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    loop {
        match aidexd::server::request_events(&root, workspace, cursor, MAX_BATCH) {
            Ok(events) => {
                let empty = events.is_empty();
                if let Err(error) = emit_events(&events, &mut output, &mut cursor) {
                    eprintln!("aidex events: cannot emit event: {error}");
                    return ExitCode::FAILURE;
                }
                if !follow {
                    return ExitCode::SUCCESS;
                }
                if empty {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            }
            Err(aidexd::server::EventRequestError::Unavailable(error)) if follow => {
                eprintln!("aidex events: reconnecting after error: {error}");
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(error) => {
                eprintln!("aidex events: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
}

fn emit_events<W: std::io::Write>(
    events: &[Event],
    output: &mut W,
    acknowledged: &mut u64,
) -> std::io::Result<()> {
    for event in events {
        serde_json::to_writer(&mut *output, event).map_err(std::io::Error::other)?;
        output.write_all(b"\n")?;
        output.flush()?;
        *acknowledged = event.sequence;
    }
    Ok(())
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Some(Commands::Chat { options, prompt }) => launch_omp(&options, &prompt),
        Some(Commands::Run { options, prompt }) => launch_sdk(&options, &prompt),
        Some(Commands::Agent {
            command: AgentCommands::List,
        }) => list_agents(),
        Some(Commands::Init { drive }) => drive_init(&drive),
        Some(Commands::Status { drive }) => drive_health(drive.as_deref()),
        Some(Commands::Daemon { command }) => daemon_command(command),
        Some(Commands::Events {
            workspace,
            after,
            drive,
            follow,
        }) => events_command(&workspace, after, drive.as_deref(), follow),
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

    let drive_ok = match resolve_root(None) {
        Ok(root) => match drive_status(&root) {
            Ok(status) => {
                print_drive_status(
                    &status.root,
                    status.schema_version,
                    status.writable,
                    status.free_bytes,
                );
                status.writable
            }
            Err(error) => {
                print_drive_error(&root, &error);
                false
            }
        },
        Err(error) => {
            eprintln!("Drive: unavailable ({error})");
            false
        }
    };
    let runtime_ok = match omp_version() {
        Ok(version) => {
            println!("Oh My Pi: available ({version})");
            true
        }
        Err(error) => {
            println!("Oh My Pi: unavailable ({error})");
            print_runtime_install_hint();
            false
        }
    };
    if drive_ok && runtime_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
#[cfg(test)]
mod tests {
    use super::*;
    use aidex_events::{EVENT_VERSION, EventKind};

    struct TestWriter {
        bytes: Vec<u8>,
        flushes: usize,
        fail_flush: Option<usize>,
    }

    impl std::io::Write for TestWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            if self.fail_flush == Some(self.flushes) {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "consumer disconnected",
                ))
            } else {
                Ok(())
            }
        }
    }

    fn event(sequence: u64) -> Event {
        Event {
            version: EVENT_VERSION,
            sequence,
            workspace_id: "workspace".into(),
            session_id: Some("session".into()),
            kind: EventKind::SessionStarted,
        }
    }

    #[test]
    fn cli_acknowledges_only_successfully_flushed_events() {
        let events = [event(4), event(5)];
        let mut writer = TestWriter {
            bytes: Vec::new(),
            flushes: 0,
            fail_flush: Some(2),
        };
        let mut cursor = 3;

        assert!(emit_events(&events, &mut writer, &mut cursor).is_err());
        assert_eq!(cursor, 4);
        let lines = std::str::from_utf8(&writer.bytes)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert_eq!(serde_json::from_str::<Event>(lines[0]).unwrap().sequence, 4);
    }

    #[test]
    fn cli_replay_batch_advances_cursor_in_event_order() {
        let events = [event(1), event(2), event(3)];
        let mut writer = TestWriter {
            bytes: Vec::new(),
            flushes: 0,
            fail_flush: None,
        };
        let mut cursor = 0;

        emit_events(&events, &mut writer, &mut cursor).unwrap();

        assert_eq!(cursor, 3);
        let emitted = std::str::from_utf8(&writer.bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Event>(line).unwrap().sequence)
            .collect::<Vec<_>>();
        assert_eq!(emitted, [1, 2, 3]);
    }
}
