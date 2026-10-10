//! Local model commands: `aidex model import|verify|remove|load|unload|loaded|benchmark|run`.
//! Paths are made absolute here because the daemon has its own working directory.

use crate::{
    commands::workspace::ephemeral,
    exit::{CliError, Exit},
    output::Output,
};
use aidex_inference::{
    BenchmarkReport, ImportPhase, LoadReport, RegistryEntry, RemovedModel, VerificationMethod,
    Verified,
};
use aidexd::{
    Client, ClientError,
    methods::inference::{
        IMPORT_DONE, IMPORT_FAILED, IMPORT_PROGRESS, ImportProgressData, LoadedList,
        MODEL_BENCHMARK, MODEL_IMPORT, MODEL_LOAD, MODEL_LOADED, MODEL_REMOVE, MODEL_RUN,
        MODEL_UNLOAD, MODEL_VERIFY, RUN_DELTA, RUN_DONE, RUN_FAILED, RunDone, StreamFailure,
        Unloaded,
    },
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

/// Upper bound for calls that hash or load multi-gigabyte models.
pub(crate) const LONG_CALL: Duration = Duration::from_secs(4 * 60 * 60);
/// Longest silence tolerated on a stream before giving up.
const STREAM_IDLE: Duration = Duration::from_secs(30 * 60);

/// An absolute, existing path for the daemon.
pub(crate) fn absolute_file(path: &Path, what: &str) -> Result<PathBuf, CliError> {
    std::fs::canonicalize(path).map_err(|error| {
        CliError::new(
            Exit::NotFound,
            format!("cannot find {what} {}: {error}", path.display()),
        )
    })
}

/// A blocking call with a long timeout, decoded as `T`.
pub(crate) fn long_call<T: DeserializeOwned>(
    client: &mut Client,
    method: &str,
    params: Value,
) -> Result<T, CliError> {
    let value = client
        .call_with_timeout(method, params, LONG_CALL)
        .map_err(with_advice)?;
    serde_json::from_value(value).map_err(|error| {
        CliError::new(
            Exit::Failure,
            format!("unexpected `{method}` result: {error}"),
        )
    })
}

/// The error a `*.failed` stream notification carries.
fn stream_error(data: Value) -> CliError {
    match serde_json::from_value::<StreamFailure>(data) {
        Ok(failure) => with_advice(ClientError::Rpc {
            code: failure.error.code,
            message: failure.error.message,
            data: failure.error.data,
        }),
        Err(error) => CliError::new(Exit::Failure, format!("malformed failure: {error}")),
    }
}

/// A CLI error that carries the daemon's `data.advice` as its hint.
pub(crate) fn with_advice(error: ClientError) -> CliError {
    let advice = match &error {
        ClientError::Rpc {
            data: Some(data), ..
        } => data["advice"].as_str().map(str::to_owned),
        _ => None,
    };
    let cli = CliError::from(error);
    match advice {
        Some(advice) => cli.with_hint(advice),
        None => cli,
    }
}

fn phase_label(phase: ImportPhase) -> &'static str {
    match phase {
        ImportPhase::Inspecting => "inspecting",
        ImportPhase::Copying => "copying",
        ImportPhase::Verifying => "verifying the copy",
        ImportPhase::Registering => "registering",
    }
}

fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

pub struct ImportArgs {
    pub file: PathBuf,
    pub id: Option<String>,
    pub name: Option<String>,
    pub license: Option<String>,
    pub license_file: Option<PathBuf>,
    pub source: Option<String>,
    pub revision: Option<String>,
    pub sha256: Option<String>,
}

pub fn import(client: &mut Client, args: ImportArgs, output: Output) -> Result<Exit, CliError> {
    let path = absolute_file(&args.file, "model file")?;
    let license_file = args
        .license_file
        .as_deref()
        .map(|file| absolute_file(file, "licence file"))
        .transpose()?;
    let params = json!({
        "path": path,
        "id": args.id,
        "display_name": args.name,
        "license": args.license,
        "license_file": license_file,
        "source": args.source,
        "revision": args.revision,
        "sha256": args.sha256,
    });
    let mut subscription = client
        .subscribe(MODEL_IMPORT, params)
        .map_err(with_advice)?;
    let mut last_phase = None;
    loop {
        let Some(notification) = subscription.next_timeout(STREAM_IDLE)? else {
            return Err(CliError::new(
                Exit::Failure,
                "the import stream ended without a result",
            ));
        };
        match notification.method.as_str() {
            IMPORT_PROGRESS => {
                if output.is_json() {
                    continue;
                }
                if let Ok(progress) =
                    serde_json::from_value::<ImportProgressData>(notification.params)
                {
                    if last_phase != Some(progress.phase) || progress.total_bytes > 0 {
                        eprint!(
                            "\r{:<20} {:>6} / {} MiB",
                            phase_label(progress.phase),
                            mib(progress.done_bytes),
                            mib(progress.total_bytes)
                        );
                        let _ = std::io::stderr().flush();
                    }
                    last_phase = Some(progress.phase);
                }
            }
            IMPORT_DONE => {
                if !output.is_json() && last_phase.is_some() {
                    eprintln!();
                }
                let entry: RegistryEntry =
                    serde_json::from_value(notification.params["entry"].clone()).map_err(
                        |error| CliError::new(Exit::Failure, format!("unexpected result: {error}")),
                    )?;
                output.emit(&entry, || print_entry(&entry))?;
                return Ok(Exit::Success);
            }
            IMPORT_FAILED => {
                if !output.is_json() && last_phase.is_some() {
                    eprintln!();
                }
                return Err(stream_error(notification.params));
            }
            _ => {}
        }
    }
}

fn print_entry(entry: &RegistryEntry) {
    println!("Imported local/{} ({})", entry.id, entry.display_name);
    println!(
        "  {} GGUF {}, {} MiB, context {} tokens",
        entry.architecture,
        entry
            .quantization
            .as_deref()
            .unwrap_or("(unknown quantization)"),
        mib(entry.size_bytes),
        entry.context_length
    );
    println!("  file:    {}", entry.file);
    println!("  sha256:  {}", entry.sha256);
    println!(
        "  licence: {} ({}){}",
        entry.license.name,
        entry.license.origin,
        entry
            .license
            .file
            .as_deref()
            .map(|file| format!(", text at {file}"))
            .unwrap_or_default()
    );
    if let Some(url) = &entry.source.url {
        println!(
            "  source:  {url}{}",
            entry
                .source
                .revision
                .as_deref()
                .map(|revision| format!(" @ {revision}"))
                .unwrap_or_default()
        );
    }
    println!(
        "  RAM:     {} MiB minimum, {} MiB recommended",
        mib(entry.memory.min_ram_bytes),
        mib(entry.memory.recommended_ram_bytes)
    );
}

pub fn verify(
    client: &mut Client,
    id: &str,
    force: bool,
    output: Output,
) -> Result<Exit, CliError> {
    let verified: Verified = long_call(client, MODEL_VERIFY, json!({"id": id, "force": force}))?;
    output.emit(&verified, || {
        let how = match verified.method {
            VerificationMethod::FullHash => "full SHA-256 now",
            VerificationMethod::Cached => "cached full hash of the unchanged file",
        };
        println!(
            "Verified local/{} ({how}, {} ms): {}",
            verified.id, verified.duration_ms, verified.sha256
        );
    })?;
    Ok(Exit::Success)
}

pub fn remove(client: &mut Client, id: &str, yes: bool, output: Output) -> Result<Exit, CliError> {
    let question = format!("Delete local model {id} and the model file AiDEX imported for it?");
    if !ephemeral::confirm(&question, yes, output)? {
        return Ok(ephemeral::cancelled(output));
    }
    let removed: RemovedModel = client
        .call_as(MODEL_REMOVE, json!({"id": id, "confirm": true}))
        .map_err(with_advice)?;
    output.emit(&removed, || {
        println!("Removed local/{}", removed.id);
        for path in &removed.deleted {
            println!("  deleted {path}");
        }
        for path in &removed.kept {
            println!("  kept    {path} (not created by AiDEX)");
        }
    })?;
    Ok(Exit::Success)
}

pub struct LoadArgs {
    pub id: String,
    pub context: Option<u32>,
    pub cpu_only: bool,
    pub no_mmap: bool,
    pub probe_gpu: bool,
}

impl LoadArgs {
    pub(crate) fn params(&self) -> Value {
        json!({
            "id": self.id,
            "context": self.context,
            "cpu_only": self.cpu_only,
            "mmap": if self.no_mmap { Some(false) } else { None },
            "probe_gpu": self.probe_gpu,
        })
    }
}

pub fn load(client: &mut Client, args: &LoadArgs, output: Output) -> Result<Exit, CliError> {
    let report: LoadReport = long_call(client, MODEL_LOAD, args.params())?;
    output.emit(&report, || {
        if report.already_loaded {
            println!("local/{} is already loaded", report.model);
        } else {
            println!(
                "Loaded local/{} in {} ms (pid {}, 127.0.0.1:{})",
                report.model, report.server.load_ms, report.server.pid, report.server.port
            );
        }
        crate::commands::runtime::print_plan(&report.plan);
        println!("Log: {}", report.server.log);
    })?;
    Ok(Exit::Success)
}

pub fn unload(client: &mut Client, id: &str, output: Output) -> Result<Exit, CliError> {
    let unloaded: Unloaded = client
        .call_as(MODEL_UNLOAD, json!({"id": id}))
        .map_err(with_advice)?;
    output.emit(&unloaded, || {
        if unloaded.unloaded {
            println!("Unloaded local/{}", unloaded.model);
        } else {
            println!("local/{} was not loaded", unloaded.model);
        }
    })?;
    Ok(Exit::Success)
}

pub fn loaded(client: &mut Client, output: Output) -> Result<Exit, CliError> {
    let list: LoadedList = client.call_as(MODEL_LOADED, Value::Null)?;
    output.emit(&list, || {
        if list.models.is_empty() {
            println!("No local model is loaded.");
        }
        for model in &list.models {
            println!(
                "local/{}  ctx {}  {} threads  {} GPU layers  pid {}  idle {} s  in flight {}",
                model.model,
                model.plan.ctx_size,
                model.plan.threads,
                model.plan.gpu_layers,
                model.server.pid,
                model.idle_ms / 1000,
                model.in_flight
            );
        }
    })?;
    Ok(Exit::Success)
}

pub fn benchmark(
    client: &mut Client,
    id: &str,
    force: bool,
    output: Output,
) -> Result<Exit, CliError> {
    let report: BenchmarkReport =
        long_call(client, MODEL_BENCHMARK, json!({"id": id, "force": force}))?;
    output.emit(&report, || {
        println!(
            "local/{}{}",
            report.model,
            if report.cached {
                " (cached for this host; --force measures again)"
            } else {
                ""
            }
        );
        println!();
        println!("Load:        {:.1} s", report.load_ms as f64 / 1000.0);
        println!(
            "Prompt:      {:.0} tok/s ({} tokens)",
            report.prompt_tokens_per_second, report.prompt_tokens
        );
        println!(
            "Generate:    {:.1} tok/s ({} tokens)",
            report.generation_tokens_per_second, report.generated_tokens
        );
        println!(
            "GPU offload: {} layers ({})",
            report.gpu_layers, report.variant
        );
        match report.ram_bytes {
            Some(bytes) => println!("RAM:         {:.1} GiB", bytes as f64 / (1u64 << 30) as f64),
            None => println!("RAM:         unknown"),
        }
        println!();
        println!(
            "Recommended: {}",
            if report.recommended { "YES" } else { "NO" }
        );
        println!("Host:        {}", report.host_fingerprint);
    })?;
    Ok(Exit::Success)
}

pub struct RunArgs {
    pub id: String,
    pub prompt: String,
    pub system: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

pub fn run(client: &mut Client, args: RunArgs, output: Output) -> Result<Exit, CliError> {
    let params = json!({
        "id": args.id,
        "prompt": args.prompt,
        "system": args.system,
        "max_tokens": args.max_tokens,
        "temperature": args.temperature,
    });
    let mut subscription = client.subscribe(MODEL_RUN, params).map_err(with_advice)?;
    let mut printed = false;
    loop {
        let Some(notification) = subscription.next_timeout(STREAM_IDLE)? else {
            return Err(CliError::new(
                Exit::Failure,
                "the run stream ended without a result",
            ));
        };
        match notification.method.as_str() {
            RUN_DELTA if !output.is_json() => {
                if let Some(text) = notification.params["text"].as_str() {
                    print!("{text}");
                    let _ = std::io::stdout().flush();
                    printed = true;
                }
            }
            RUN_DONE => {
                let done: RunDone =
                    serde_json::from_value(notification.params).map_err(|error| {
                        CliError::new(Exit::Failure, format!("unexpected result: {error}"))
                    })?;
                output.emit(&done, || {
                    if printed {
                        println!();
                    }
                    let tokens = done.usage.completion_tokens.unwrap_or(0);
                    eprintln!(
                        "[{} prompt + {tokens} completion tokens, {:.1} s, {}]",
                        done.usage.prompt_tokens.unwrap_or(0),
                        done.elapsed_ms as f64 / 1000.0,
                        serde_json::to_value(done.finish_reason)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .unwrap_or_default()
                    );
                })?;
                return Ok(Exit::Success);
            }
            RUN_FAILED => {
                if printed && !output.is_json() {
                    println!();
                }
                return Err(stream_error(notification.params));
            }
            _ => {}
        }
    }
}
