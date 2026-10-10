//! `aidex inference ...`: managed local GGUF inference over the daemon's
//! `inference.*` methods (catalog, import, runtime install, start/stop/status,
//! model removal, cancellation, and host facts). Paths are made absolute here
//! because the daemon has its own working directory.

use crate::{
    commands::{daemon::connect, workspace::ephemeral},
    exit::{CliError, Exit},
    output::Output,
};
use aidex_inference::{Catalog, HostReport, ManagedModel, RuntimeInfo};
use aidexd::{
    Client,
    methods::inference::{
        INFERENCE_CANCEL, INFERENCE_CATALOG, INFERENCE_HOST, INFERENCE_IMPORT,
        INFERENCE_MODEL_REMOVE, INFERENCE_RUNTIME_INSTALL, INFERENCE_START, INFERENCE_STATUS,
        INFERENCE_STOP,
    },
};
use clap::{Args, Subcommand, ValueEnum};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Imports, runtime installs, and model loads hash and copy large files.
const LONG_CALL: Duration = Duration::from_secs(4 * 60 * 60);

#[derive(Debug, Args)]
pub struct InferenceArgs {
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH", global = true)]
    pub drive: Option<PathBuf>,
    #[command(subcommand)]
    pub command: InferenceCommand,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum BackendArg {
    Auto,
    Cpu,
    Gpu,
}

impl BackendArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum InferenceCommand {
    /// Show whether a local model is loaded, its settings, and how they were chosen.
    Status,
    /// List managed models, the installed runtime, and the saved start settings.
    Catalog,
    /// Copy a GGUF model onto the drive (validated and hashed; never overwrites).
    Import {
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Display name (defaults to the file name).
        #[arg(long)]
        name: Option<String>,
    },
    /// Install the llama.cpp runtime from a verified distribution directory
    /// containing `runtime-manifest.json` (default: the drive's components directory).
    InstallRuntime {
        #[arg(value_name = "DIRECTORY")]
        source: Option<PathBuf>,
    },
    /// Start the managed llama-server for a model (`auto` settings are host-aware).
    Start {
        #[arg(value_name = "MODEL_ID")]
        model_id: String,
        #[arg(long, value_enum, default_value = "auto")]
        backend: BackendArg,
        /// Context tokens (default: sized to the model and available RAM).
        #[arg(long, value_name = "TOKENS")]
        context: Option<u32>,
        /// Threads (default: physical cores).
        #[arg(long)]
        threads: Option<u32>,
    },
    /// Stop the managed llama-server (also cancels a running operation).
    Stop,
    /// Delete a managed model's files (it must not be loaded).
    Remove {
        #[arg(value_name = "MODEL_ID")]
        model_id: String,
        /// Do not ask for confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Cancel the running import, install, or start.
    Cancel,
    /// Show the memory, CPU, and GPU facts start settings are planned with.
    Host {
        /// Run the opt-in GPU probe (may execute `nvidia-smi`).
        #[arg(long)]
        probe_gpu: bool,
    },
}

impl InferenceCommand {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Status => "aidex inference status",
            Self::Catalog => "aidex inference catalog",
            Self::Import { .. } => "aidex inference import",
            Self::InstallRuntime { .. } => "aidex inference install-runtime",
            Self::Start { .. } => "aidex inference start",
            Self::Stop => "aidex inference stop",
            Self::Remove { .. } => "aidex inference remove",
            Self::Cancel => "aidex inference cancel",
            Self::Host { .. } => "aidex inference host",
        }
    }
}

fn absolute(path: &Path, what: &str) -> Result<PathBuf, CliError> {
    std::fs::canonicalize(path).map_err(|error| {
        CliError::new(
            Exit::NotFound,
            format!("cannot find {what} {}: {error}", path.display()),
        )
    })
}

fn long_call<T: DeserializeOwned>(
    client: &mut Client,
    method: &str,
    params: Value,
) -> Result<T, CliError> {
    let value = client.call_with_timeout(method, params, LONG_CALL)?;
    serde_json::from_value(value).map_err(|error| {
        CliError::new(
            Exit::Failure,
            format!("unexpected `{method}` result: {error}"),
        )
    })
}

fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

fn gib(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || "unknown".into(),
        |bytes| format!("{:.1} GiB", bytes as f64 / (1u64 << 30) as f64),
    )
}

pub fn run(args: InferenceArgs, output: Output) -> Result<Exit, CliError> {
    let mut client = connect(args.drive.as_deref())?;
    match args.command {
        InferenceCommand::Status => {
            let status: Value = client.call_as(INFERENCE_STATUS, Value::Null)?;
            output.emit(&status, || print_status(&status))?;
        }
        InferenceCommand::Catalog => {
            let catalog: Catalog = client.call_as(INFERENCE_CATALOG, Value::Null)?;
            output.emit(&catalog, || print_catalog(&catalog))?;
        }
        InferenceCommand::Import { file, name } => {
            let source = absolute(&file, "model file")?;
            let model: ManagedModel = long_call(
                &mut client,
                INFERENCE_IMPORT,
                json!({ "source": source, "name": name }),
            )?;
            output.emit(&model, || {
                println!("Imported {} ({})", model.id, model.name);
                print_model(&model);
            })?;
        }
        InferenceCommand::InstallRuntime { source } => {
            let source = source
                .as_deref()
                .map(|source| absolute(source, "runtime distribution"))
                .transpose()?;
            let runtime: RuntimeInfo = long_call(
                &mut client,
                INFERENCE_RUNTIME_INSTALL,
                json!({ "source": source }),
            )?;
            output.emit(&runtime, || {
                println!(
                    "Installed llama.cpp {} for {} at {}",
                    runtime.release, runtime.platform, runtime.path
                );
                print_runtime(&runtime);
            })?;
        }
        InferenceCommand::Start {
            model_id,
            backend,
            context,
            threads,
        } => {
            let status: Value = long_call(
                &mut client,
                INFERENCE_START,
                json!({
                    "model_id": model_id,
                    "backend": backend.as_str(),
                    "context_window": context,
                    "threads": threads,
                }),
            )?;
            output.emit(&status, || print_status(&status))?;
        }
        InferenceCommand::Stop => {
            let status: Value = long_call(&mut client, INFERENCE_STOP, Value::Null)?;
            output.emit(&status, || print_status(&status))?;
        }
        InferenceCommand::Remove { model_id, yes } => {
            let question = format!("Delete managed model {model_id} and its files from the drive?");
            if !ephemeral::confirm(&question, yes, output)? {
                return Ok(ephemeral::cancelled(output));
            }
            let removed: Value =
                client.call_as(INFERENCE_MODEL_REMOVE, json!({ "id": model_id }))?;
            output.emit(&removed, || println!("Removed {model_id}"))?;
        }
        InferenceCommand::Cancel => {
            let cancelled: Value = client.call_as(INFERENCE_CANCEL, Value::Null)?;
            output.emit(&cancelled, || println!("Cancellation requested"))?;
        }
        InferenceCommand::Host { probe_gpu } => {
            let host: HostReport =
                client.call_as(INFERENCE_HOST, json!({ "probe_gpu": probe_gpu }))?;
            output.emit(&host, || {
                let resources = &host.resources;
                println!(
                    "CPU: {} ({} physical / {} logical cores)",
                    resources.cpu_brand.as_deref().unwrap_or("unknown"),
                    resources
                        .physical_cores
                        .map_or_else(|| "?".into(), |cores| cores.to_string()),
                    resources
                        .logical_cores
                        .map_or_else(|| "?".into(), |cores| cores.to_string())
                );
                println!(
                    "RAM: {} available of {}",
                    gib(resources.available_ram_bytes),
                    gib(resources.total_ram_bytes)
                );
                println!("GPU: {}", host.gpu.summary());
            })?;
        }
    }
    Ok(Exit::Success)
}

fn print_model(model: &ManagedModel) {
    println!(
        "  {}  {} MiB  GGUF v{}  context {}  {}",
        model.path,
        mib(model.size_bytes),
        model.gguf_version,
        model
            .context_window
            .map_or_else(|| "unknown".into(), |context| context.to_string()),
        model.availability.as_str()
    );
    println!("  sha256 {}", model.sha256);
}

fn print_runtime(runtime: &RuntimeInfo) {
    println!(
        "  {} ({}), checksums {}, GPU {}",
        runtime.executable,
        runtime.path,
        if runtime.verified {
            "verified"
        } else {
            "FAILED"
        },
        if runtime.gpu_supported {
            "declared"
        } else {
            "not supported"
        }
    );
}

fn print_catalog(catalog: &Catalog) {
    if catalog.models.is_empty() {
        println!("No managed models (import one with `aidex inference import <FILE>`).");
    }
    for model in &catalog.models {
        println!("{} ({})", model.id, model.name);
        print_model(model);
    }
    match &catalog.runtime {
        Some(runtime) => {
            println!(
                "Runtime: llama.cpp {} for {}",
                runtime.release, runtime.platform
            );
            print_runtime(runtime);
        }
        None => println!(
            "Runtime: not installed (install one with `aidex inference install-runtime <DIRECTORY>`)"
        ),
    }
    if let Some(settings) = &catalog.settings {
        println!("Saved start settings: {}", settings.model_id);
    }
}

fn print_status(status: &Value) {
    let text = |key: &str| status[key].as_str().unwrap_or("-").to_owned();
    println!("State: {}", text("state"));
    if let Some(model) = status["model_id"].as_str() {
        println!("Model: {model} ({})", text("model_name"));
    }
    if let Some(error) = status["startup_error"].as_str() {
        println!("Startup error: {error}");
    }
    if let Some(endpoint) = status["endpoint"].as_str() {
        println!(
            "Server: {endpoint} (pid {}), {} threads, context {}",
            status["pid"], status["threads"], status["context_window"]
        );
    }
    if let Some(plan) = status["plan"].as_object() {
        if let Some(estimate) = plan["estimated_ram_bytes"].as_u64() {
            println!("Estimated RAM: {} MiB", mib(estimate));
        }
        println!("Plan:");
        for reason in plan["reasons"].as_array().into_iter().flatten() {
            if let Some(reason) = reason.as_str() {
                println!("  - {reason}");
            }
        }
    }
    for limitation in status["limitations"].as_array().into_iter().flatten() {
        if let Some(limitation) = limitation.as_str() {
            println!("Limitation: {limitation}");
        }
    }
}
