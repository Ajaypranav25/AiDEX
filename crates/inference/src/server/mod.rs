//! A managed `llama-server` process (spec §96): launched from a verified
//! runtime through `aidex-sandbox` (Brokered level: process-tree containment
//! and a cleared environment), bound to `127.0.0.1` on a free port, protected by
//! a random 256-bit key passed through the child's environment
//! (`LLAMA_API_KEY`, never argv), and torn down with its whole process tree.

mod http;
mod logs;

pub use http::CompletionTimings;

use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use aidex_core::CancelToken;
use aidex_providers::{
    BaseUrl, ChatRequest, ChatResponse, HttpOptions, OpenAiClient, ProviderError, SecretBytes,
    StreamEvent,
};
use aidex_sandbox::{
    CommandSpec, ManagedProcess, Mount, NetworkMode, ResourceLimits, Sandbox, SandboxLevel,
    SandboxRequest,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    InferenceError, LaunchPlan, LocalModelId, Result,
    fsutil::{self, display, plain_path},
    runtime::InstalledRuntime,
};
use logs::{LogPump, ServerLog, drain};

/// Environment variable llama-server reads its API key from.
pub const API_KEY_ENV: &str = "LLAMA_API_KEY";
/// Launch attempts when the chosen port was taken in the meantime.
const PORT_ATTEMPTS: usize = 3;

/// Timeouts and limits of managed servers.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// How long a server may take to load its model and answer `/health`.
    pub health_timeout: Duration,
    pub health_interval: Duration,
    /// Bytes written to one server log at most.
    pub log_limit_bytes: u64,
    /// Logs kept per model (older ones are deleted).
    pub logs_kept: usize,
    /// Grace period between killing the tree and giving up on its exit.
    pub stop_grace: Duration,
    /// HTTP limits for chat requests.
    pub http: HttpOptions,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            health_timeout: Duration::from_secs(180),
            health_interval: Duration::from_millis(100),
            log_limit_bytes: 8 * 1024 * 1024,
            logs_kept: 5,
            stop_grace: Duration::from_secs(10),
            http: HttpOptions::default(),
        }
    }
}

/// What a launch needs.
pub(crate) struct ServerLaunch<'a> {
    pub runtime: &'a InstalledRuntime,
    pub model_id: &'a LocalModelId,
    pub model_path: &'a Path,
    pub plan: &'a LaunchPlan,
    pub logs_dir: &'a Path,
    /// Parent of this server's scratch directory (`tmp/inference`).
    pub scratch_root: &'a Path,
}

/// Facts about a running server that are safe to show (no key).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerInfo {
    pub process_id: String,
    pub pid: u32,
    pub port: u16,
    /// Time from spawn to a healthy `/health`.
    pub load_ms: u64,
    pub log: String,
}

/// A running llama-server. Dropping it kills the process tree.
pub struct ManagedServer {
    process: Arc<ManagedProcess>,
    port: u16,
    key: SecretBytes,
    client: OpenAiClient,
    http: http::HealthClient,
    log: Arc<ServerLog>,
    pump: Mutex<LogPump>,
    stopped: AtomicBool,
    load_time: Duration,
    scratch: PathBuf,
    stop_grace: Duration,
}

impl std::fmt::Debug for ManagedServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagedServer")
            .field("pid", &self.process.pid())
            .field("port", &self.port)
            .field("key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// llama-server arguments for `plan` (the key is not among them).
pub(crate) fn server_args(plan: &LaunchPlan, model_path: &Path, port: u16) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--model".into(),
        plain_path(model_path).display().to_string(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--ctx-size".into(),
        plan.ctx_size.to_string(),
        "--threads".into(),
        plan.threads.to_string(),
        "--threads-batch".into(),
        plan.threads.to_string(),
        "--batch-size".into(),
        plan.batch_size.to_string(),
        "--ubatch-size".into(),
        plan.ubatch_size.to_string(),
        "--n-gpu-layers".into(),
        plan.gpu_layers.to_string(),
        "--parallel".into(),
        "1".into(),
        "--load-mode".into(),
        if plan.mmap { "mmap" } else { "none" }.into(),
    ];
    args.extend(
        ["--jinja", "--no-webui", "--no-slots", "--offline"]
            .into_iter()
            .map(str::to_owned),
    );
    args
}

/// A loopback port that was free a moment ago.
fn free_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| InferenceError::io("cannot reserve a loopback port", &error))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| InferenceError::io("cannot read the reserved port", &error))
}

fn generate_key() -> Result<Zeroizing<String>> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    getrandom::fill(bytes.as_mut()).map_err(|error| InferenceError::Io {
        context: "operating-system randomness is unavailable".into(),
        message: error.to_string(),
    })?;
    Ok(Zeroizing::new(fsutil::hex(bytes.as_ref())))
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl ManagedServer {
    /// Launch and wait until `/health` reports ready.
    pub(crate) fn start(
        sandbox: &Sandbox,
        launch: &ServerLaunch<'_>,
        config: &ServerConfig,
        cancel: &CancelToken,
    ) -> Result<Self> {
        let mut last_error = None;
        for _ in 0..PORT_ATTEMPTS {
            match Self::start_once(sandbox, launch, config, cancel) {
                Ok(server) => return Ok(server),
                Err((error, port_conflict)) => {
                    if !port_conflict {
                        return Err(error);
                    }
                    last_error = Some(error);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| InferenceError::ServerExited {
            code: None,
            detail: "no free loopback port".into(),
            log: None,
        }))
    }

    /// One launch attempt; the flag says whether a port conflict caused the failure.
    fn start_once(
        sandbox: &Sandbox,
        launch: &ServerLaunch<'_>,
        config: &ServerConfig,
        cancel: &CancelToken,
    ) -> std::result::Result<Self, (InferenceError, bool)> {
        let fatal = |error: InferenceError| (error, false);
        let port = free_port().map_err(fatal)?;
        let key = generate_key().map_err(fatal)?;
        let model_dir = launch.model_path.parent().ok_or_else(|| {
            fatal(InferenceError::corrupt(
                launch.model_path,
                "has no directory",
            ))
        })?;
        let scratch = launch.scratch_root.join(format!(
            "{}-{}",
            launch.model_id,
            fsutil::random_suffix().map_err(fatal)?
        ));
        fs::create_dir_all(&scratch).map_err(|error| {
            fatal(InferenceError::io(
                format!("cannot create {}", display(&scratch)),
                &error,
            ))
        })?;
        let cleanup = |error: InferenceError| {
            let _ = fs::remove_dir_all(&scratch);
            (error, false)
        };
        let log = Arc::new(
            ServerLog::create(
                launch.logs_dir,
                launch.model_id,
                config.log_limit_bytes,
                config.logs_kept,
                &key,
            )
            .map_err(cleanup)?,
        );
        let args = server_args(launch.plan, launch.model_path, port);
        log.note(&format!(
            "launching {} {} (runtime {}, {} threads, ctx {}, {} GPU layers)",
            launch.runtime.manifest.server_binary,
            args.join(" "),
            launch.runtime.manifest.catalog_id,
            launch.plan.threads,
            launch.plan.ctx_size,
            launch.plan.gpu_layers
        ));
        let request = SandboxRequest::new(SandboxLevel::Brokered, &launch.runtime.dir, &scratch)
            .with_mount(Mount::read_only(&launch.runtime.dir))
            .with_mount(Mount::read_only(model_dir))
            // llama-server must listen on loopback; it gets no network features
            // (`--offline`) and every request needs the per-launch key.
            .with_network(NetworkMode::Normal)
            .with_limits(ResourceLimits {
                max_output_bytes: 1024 * 1024,
                ..ResourceLimits::default()
            })
            .with_env(API_KEY_ENV, key.as_str());
        let plan = sandbox
            .negotiate(&request)
            .map_err(|error| cleanup(error.into()))?;
        let started = Instant::now();
        let process = sandbox
            .spawn(
                &plan,
                CommandSpec::new(launch.runtime.server_path()).args(args),
            )
            .map_err(|error| cleanup(error.into()))?;
        let process = Arc::new(process);
        let mut pump = LogPump::start(Arc::clone(&process), Arc::clone(&log)).map_err(cleanup)?;
        let base = format!("http://127.0.0.1:{port}");
        let http = http::HealthClient::new(&base);
        let failed = |pump: &mut LogPump, error: InferenceError, conflict: bool| {
            let _ = process.kill_tree();
            let _ = process.wait_timeout(config.stop_grace);
            drain(&process, &log);
            pump.finish(&log);
            let _ = fs::remove_dir_all(&scratch);
            (error, conflict)
        };
        let deadline = started + config.health_timeout;
        loop {
            if cancel.is_stopped() {
                log.note("launch cancelled");
                return Err(failed(&mut pump, InferenceError::Cancelled, false));
            }
            if let Ok(Some(status)) = process.try_wait() {
                drain(&process, &log);
                let conflict =
                    log.tail_contains("bind") || log.tail_contains("address already in use");
                log.note(&format!("exited before ready: code {:?}", status.code));
                let error = InferenceError::ServerExited {
                    code: status.code,
                    detail: {
                        let tail = log.tail_detail();
                        if tail.is_empty() {
                            "no output".into()
                        } else {
                            tail
                        }
                    },
                    log: Some(log.path().to_path_buf()),
                };
                return Err(failed(&mut pump, error, conflict));
            }
            if http.is_ready() {
                break;
            }
            if Instant::now() >= deadline {
                log.note("health timeout");
                let error = InferenceError::ServerTimeout {
                    after: config.health_timeout,
                    log: Some(log.path().to_path_buf()),
                };
                return Err(failed(&mut pump, error, false));
            }
            thread::sleep(config.health_interval);
        }
        let load_time = started.elapsed();
        log.note(&format!("ready after {} ms", millis(load_time)));
        let base_url = BaseUrl::parse(&format!("{base}/v1")).map_err(|error| {
            failed(
                &mut pump,
                InferenceError::InvalidArgument(error.to_string()),
                false,
            )
        })?;
        Ok(Self {
            process,
            port,
            key: SecretBytes::new(key.as_bytes().to_vec()),
            client: OpenAiClient::new(base_url, config.http),
            http,
            log,
            pump: Mutex::new(pump),
            stopped: AtomicBool::new(false),
            load_time,
            scratch,
            stop_grace: config.stop_grace,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn load_time(&self) -> Duration {
        self.load_time
    }

    pub fn log_path(&self) -> &Path {
        self.log.path()
    }

    pub fn is_running(&self) -> bool {
        self.process.is_running()
    }

    pub fn info(&self) -> ServerInfo {
        ServerInfo {
            process_id: self.process.id().to_string(),
            pid: self.process.pid(),
            port: self.port,
            load_ms: millis(self.load_time),
            log: display(self.log.path()),
        }
    }

    /// Resident memory of the server's process tree.
    pub fn memory_bytes(&self) -> Option<u64> {
        let mut pids = self.process.member_pids().unwrap_or_default();
        if pids.is_empty() {
            pids.push(self.process.pid());
        }
        aidex_host::resident_memory_bytes(&pids)
    }

    /// Chat through the server's OpenAI-compatible API.
    pub fn chat(
        &self,
        request: &ChatRequest,
        on_event: &mut dyn FnMut(&StreamEvent),
        cancel: &CancelToken,
    ) -> std::result::Result<ChatResponse, ProviderError> {
        if !self.is_running() {
            return Err(ProviderError::unavailable(
                "the local llama-server stopped; it will be restarted on the next request",
            ));
        }
        self.client.chat(request, Some(&self.key), on_event, cancel)
    }

    /// One non-streaming completion with llama-server's own timings.
    pub fn timed_completion(&self, prompt: &str, max_tokens: u32) -> Result<CompletionTimings> {
        self.http.timed_completion(&self.key, prompt, max_tokens)
    }

    /// Kill the process tree and wait for it (idempotent). In-flight requests fail.
    pub fn stop(&self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.process.kill_tree();
        let status = self.process.wait_timeout(self.stop_grace);
        drain(&self.process, &self.log);
        match status {
            Ok(Some(status)) => self.log.note(&format!("stopped (code {:?})", status.code)),
            _ => self.log.note("stop requested; exit not confirmed"),
        }
        if let Ok(mut pump) = self.pump.lock() {
            pump.finish(&self.log);
        }
        let _ = fs::remove_dir_all(&self.scratch);
    }
}

impl Drop for ManagedServer {
    fn drop(&mut self) {
        self.stop();
    }
}
