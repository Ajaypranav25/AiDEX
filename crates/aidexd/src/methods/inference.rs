//! Local inference methods over [`aidex_inference::LocalInference`]: model
//! import (streamed progress), verification, removal, loading, unloading,
//! benchmarking, one-shot runs (streamed), and runtime install/list/plan.
//!
//! Results never contain the per-launch llama-server key. Errors carry the
//! inference error kind and advice in `data` (`{kind, advice}`).

use crate::{
    errors::{ErrorCode, RpcError},
    methods::models::provider_error,
    router::{CallContext, Router, RouterError},
    rpc::{expect_no_params, parse_params, to_result},
    services::Services,
    stream::NotificationSink,
};
use aidex_core::CancelToken;
use aidex_inference::{
    ComposeRequest, ImportOptions, ImportPhase, InferenceError, LoadOptions, LoadedSummary,
    LocalModelId,
};
use aidex_providers::{ChatMessage, ChatRequest, FinishReason, StreamEvent, Usage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub const MODEL_IMPORT: &str = "model.import";
pub const MODEL_VERIFY: &str = "model.verify";
pub const MODEL_REMOVE: &str = "model.remove";
pub const MODEL_LOAD: &str = "model.load";
pub const MODEL_UNLOAD: &str = "model.unload";
pub const MODEL_LOADED: &str = "model.loaded";
pub const MODEL_BENCHMARK: &str = "model.benchmark";
pub const MODEL_RUN: &str = "model.run";
pub const RUNTIME_LIST: &str = "runtime.list";
pub const RUNTIME_INSTALL: &str = "runtime.install";
pub const RUNTIME_SELECT: &str = "runtime.select";

/// Stream notifications of `model.import`.
pub const IMPORT_PROGRESS: &str = "model.import.progress";
pub const IMPORT_DONE: &str = "model.import.done";
pub const IMPORT_FAILED: &str = "model.import.failed";
/// Stream notifications of `model.run`.
pub const RUN_DELTA: &str = "model.run.delta";
pub const RUN_DONE: &str = "model.run.done";
pub const RUN_FAILED: &str = "model.run.failed";

/// How often import progress is reported at most.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
/// How often a blocking call checks whether its client went away.
const CANCEL_POLL: Duration = Duration::from_millis(50);
/// Longest prompt `model.run` accepts.
const MAX_PROMPT: usize = 64 * 1024;

/// Params of `model.import`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImportParams {
    /// Path of the GGUF file on the daemon's host.
    pub path: PathBuf,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub license_file: Option<PathBuf>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
}

/// Initial data of the `model.import` and `model.run` streams.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Started {
    pub started: bool,
}

/// `model.import.progress` data.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImportProgressData {
    pub phase: ImportPhase,
    pub done_bytes: u64,
    pub total_bytes: u64,
}

/// `*.failed` data: the error a plain call would have returned.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StreamFailure {
    pub error: RpcError,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IdParams {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerifyParams {
    pub id: String,
    /// Rehash even when a cached verification applies.
    #[serde(default)]
    pub force: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    pub id: String,
    /// Must be `true`: removal deletes the imported model file.
    #[serde(default)]
    pub confirm: bool,
}

/// Params of `model.load` and `runtime.select`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanParams {
    #[serde(alias = "model")]
    pub id: String,
    #[serde(default)]
    pub context: Option<u32>,
    #[serde(default)]
    pub mmap: Option<bool>,
    #[serde(default)]
    pub cpu_only: bool,
    /// Run the opt-in GPU probe (`nvidia-smi`) first.
    #[serde(default)]
    pub probe_gpu: bool,
}

impl PlanParams {
    fn options(&self) -> LoadOptions {
        LoadOptions {
            compose: ComposeRequest {
                context: self.context,
                mmap: self.mmap,
                cpu_only: self.cpu_only,
            },
            probe_gpu: self.probe_gpu,
        }
    }
}

/// Result of `model.unload`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Unloaded {
    pub model: LocalModelId,
    pub unloaded: bool,
}

/// Result of `model.loaded`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LoadedList {
    pub models: Vec<LoadedSummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkParams {
    pub id: String,
    /// Measure again even when this host has a cached result.
    #[serde(default)]
    pub force: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RunParams {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
}

/// `model.run.delta` data.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunDelta {
    pub text: String,
}

/// `model.run.done` data.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunDone {
    pub content: String,
    pub finish_reason: FinishReason,
    pub usage: Usage,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstallParams {
    /// Path of the runtime archive on the daemon's host.
    pub archive: PathBuf,
}

/// Map an inference failure to its wire error; `data` names the kind and advice.
pub fn inference_error(error: &InferenceError) -> RpcError {
    if let InferenceError::Provider(inner) = error {
        return provider_error(inner);
    }
    let code = match error {
        InferenceError::ModelNotFound { .. } => ErrorCode::NotFound,
        InferenceError::Cancelled => ErrorCode::Cancelled,
        InferenceError::OfflineRefused { .. } => ErrorCode::PolicyDenied,
        InferenceError::AlreadyExists { .. } | InferenceError::Busy { .. } => ErrorCode::Conflict,
        InferenceError::InvalidModelId { .. }
        | InferenceError::InvalidArgument(_)
        | InferenceError::Gguf(_)
        | InferenceError::IncompatibleModel { .. }
        | InferenceError::RuntimeNotInCatalog { .. }
        | InferenceError::UnsupportedArchiveFormat { .. }
        | InferenceError::UnsafeArchive { .. } => ErrorCode::InvalidArgument,
        // Integrity, platform, runtime, memory, and server failures all mean the
        // local model cannot be used right now.
        _ => ErrorCode::ModelUnavailable,
    };
    let mut data = json!({ "kind": error.kind() });
    if let Some(advice) = error.advice() {
        data["advice"] = Value::String(advice);
    }
    RpcError::new(code, error.to_string()).with_data(data)
}

fn model_id(text: &str) -> Result<LocalModelId, RpcError> {
    LocalModelId::new(text).map_err(|error| inference_error(&error))
}

/// Run `work` with a token that is cancelled when the client goes away.
fn with_cancel<T>(context: &CallContext, work: impl FnOnce(&CancelToken) -> T) -> T {
    let cancel = CancelToken::new();
    let done = AtomicBool::new(false);
    thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                if context.is_cancelled() {
                    cancel.cancel();
                    return;
                }
                thread::sleep(CANCEL_POLL);
            }
        });
        let result = work(&cancel);
        done.store(true, Ordering::SeqCst);
        result
    })
}

/// Cancel `cancel` once `sink` closes, until `done` is set.
fn watch_sink(sink: &NotificationSink, cancel: &CancelToken, done: &AtomicBool) {
    while !done.load(Ordering::SeqCst) {
        if sink.is_closed() {
            cancel.cancel();
            return;
        }
        thread::sleep(CANCEL_POLL);
    }
}

fn notify_failure(sink: &NotificationSink, method: &str, error: &InferenceError) {
    let failure = StreamFailure {
        error: inference_error(error),
    };
    if let Ok(data) = serde_json::to_value(&failure) {
        let _ = sink.notify(method, data);
    }
}

fn import_stream(services: &Services, params: ImportParams, sink: &NotificationSink) {
    let options = ImportOptions {
        id: params.id,
        display_name: params.display_name,
        license: params.license,
        license_file: params.license_file,
        source_url: params.source,
        source_revision: params.revision,
        expected_sha256: params.sha256,
    };
    let cancel = CancelToken::new();
    let done = AtomicBool::new(false);
    let result = thread::scope(|scope| {
        scope.spawn(|| watch_sink(sink, &cancel, &done));
        let mut last: Option<(ImportPhase, Instant)> = None;
        let result = services.inference().import(
            &params.path,
            &options,
            &mut |progress| {
                let due = last.is_none_or(|(phase, at)| {
                    phase != progress.phase || at.elapsed() >= PROGRESS_INTERVAL
                });
                if due {
                    last = Some((progress.phase, Instant::now()));
                    let data = ImportProgressData {
                        phase: progress.phase,
                        done_bytes: progress.done_bytes,
                        total_bytes: progress.total_bytes,
                    };
                    if let Ok(data) = serde_json::to_value(&data) {
                        let _ = sink.notify(IMPORT_PROGRESS, data);
                    }
                }
            },
            &cancel,
        );
        done.store(true, Ordering::SeqCst);
        result
    });
    match result {
        Ok(entry) => {
            if let Ok(data) = serde_json::to_value(json!({ "entry": entry })) {
                let _ = sink.notify(IMPORT_DONE, data);
            }
        }
        Err(error) => notify_failure(sink, IMPORT_FAILED, &error),
    }
}

fn run_stream(services: &Services, params: RunParams, sink: &NotificationSink) {
    let id = match LocalModelId::new(params.id.clone()) {
        Ok(id) => id,
        Err(error) => return notify_failure(sink, RUN_FAILED, &error),
    };
    let mut messages = Vec::new();
    if let Some(system) = params.system {
        messages.push(ChatMessage::system(system));
    }
    messages.push(ChatMessage::user(params.prompt));
    let mut request = ChatRequest::new(id.model_id().clone(), messages);
    request.max_tokens = params.max_tokens;
    request.temperature = params.temperature;
    let cancel = CancelToken::new();
    let done = AtomicBool::new(false);
    let started = Instant::now();
    let result = thread::scope(|scope| {
        scope.spawn(|| watch_sink(sink, &cancel, &done));
        let result = services.inference().chat(
            &id,
            &request,
            &mut |event| {
                if let StreamEvent::TextDelta { text } = event {
                    if sink.notify(RUN_DELTA, json!({ "text": text })).is_err() {
                        cancel.cancel();
                    }
                }
            },
            &cancel,
        );
        done.store(true, Ordering::SeqCst);
        result
    });
    match result {
        Ok(response) => {
            let done = RunDone {
                content: response.message.content,
                finish_reason: response.finish_reason,
                usage: response.usage,
                elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            };
            if let Ok(data) = serde_json::to_value(&done) {
                let _ = sink.notify(RUN_DONE, data);
            }
        }
        Err(error) => notify_failure(sink, RUN_FAILED, &error),
    }
}

fn spawn_stream(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), RpcError> {
    thread::Builder::new()
        .name(name.into())
        .spawn(body)
        .map(drop)
        .map_err(|error| RpcError::internal(format!("cannot start {name}: {error}")))
}

pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let shared = Arc::clone(services);
    router.register_stream(MODEL_IMPORT, move |_context, params, sink| {
        let params: ImportParams = parse_params(params)?;
        let services = Arc::clone(&shared);
        spawn_stream("aidexd-model-import", move || {
            import_stream(&services, params, &sink);
        })?;
        to_result(&Started { started: true })
    })?;
    let shared = Arc::clone(services);
    router.register(MODEL_VERIFY, move |context, params| {
        let params: VerifyParams = parse_params(params)?;
        let id = model_id(&params.id)?;
        with_cancel(context, |cancel| {
            shared
                .inference()
                .verify(&id, params.force, &mut |_| {}, cancel)
        })
        .map_err(|error| inference_error(&error))
        .and_then(|verified| to_result(&verified))
    })?;
    let shared = Arc::clone(services);
    router.register(MODEL_REMOVE, move |_context, params| {
        let params: RemoveParams = parse_params(params)?;
        let id = model_id(&params.id)?;
        if !params.confirm {
            return Err(RpcError::invalid_params(
                "removing a model deletes its imported file; pass `confirm: true`",
            ));
        }
        let removed = shared
            .inference()
            .remove(&id)
            .map_err(|error| inference_error(&error))?;
        to_result(&removed)
    })?;
    let shared = Arc::clone(services);
    router.register(MODEL_LOAD, move |context, params| {
        let params: PlanParams = parse_params(params)?;
        let id = model_id(&params.id)?;
        let options = params.options();
        with_cancel(context, |cancel| {
            shared.inference().load(&id, &options, cancel)
        })
        .map_err(|error| inference_error(&error))
        .and_then(|report| to_result(&report))
    })?;
    let shared = Arc::clone(services);
    router.register(MODEL_UNLOAD, move |_context, params| {
        let params: IdParams = parse_params(params)?;
        let id = model_id(&params.id)?;
        let unloaded = shared.inference().unload(&id);
        to_result(&Unloaded {
            model: id,
            unloaded,
        })
    })?;
    let shared = Arc::clone(services);
    router.register(MODEL_LOADED, move |_context, params| {
        expect_no_params(&params)?;
        to_result(&LoadedList {
            models: shared.inference().loaded(),
        })
    })?;
    let shared = Arc::clone(services);
    router.register(MODEL_BENCHMARK, move |context, params| {
        let params: BenchmarkParams = parse_params(params)?;
        let id = model_id(&params.id)?;
        with_cancel(context, |cancel| {
            shared.inference().benchmark(&id, params.force, cancel)
        })
        .map_err(|error| inference_error(&error))
        .and_then(|report| to_result(&report))
    })?;
    let shared = Arc::clone(services);
    router.register_stream(MODEL_RUN, move |_context, params, sink| {
        let params: RunParams = parse_params(params)?;
        if params.prompt.trim().is_empty() || params.prompt.len() > MAX_PROMPT {
            return Err(RpcError::invalid_params(
                "prompt must be non-empty and at most 64 KiB",
            ));
        }
        let services = Arc::clone(&shared);
        spawn_stream("aidexd-model-run", move || {
            run_stream(&services, params, &sink);
        })?;
        to_result(&Started { started: true })
    })?;
    let shared = Arc::clone(services);
    router.register(RUNTIME_LIST, move |_context, params| {
        expect_no_params(&params)?;
        shared
            .inference()
            .runtimes()
            .map_err(|error| inference_error(&error))
            .and_then(|inventory| to_result(&inventory))
    })?;
    let shared = Arc::clone(services);
    router.register(RUNTIME_INSTALL, move |context, params| {
        let params: InstallParams = parse_params(params)?;
        with_cancel(context, |cancel| {
            shared
                .inference()
                .install_runtime(&params.archive, &mut |_| {}, cancel)
        })
        .map_err(|error| inference_error(&error))
        .and_then(|installed| to_result(&installed))
    })?;
    let shared = Arc::clone(services);
    router.register(RUNTIME_SELECT, move |_context, params| {
        let params: PlanParams = parse_params(params)?;
        let id = model_id(&params.id)?;
        shared
            .inference()
            .plan(&id, &params.options())
            .map_err(|error| inference_error(&error))
            .and_then(|report| to_result(&report))
    })?;
    Ok(())
}
