use crate::{catalog, files, runtime, *};
use aidex_core::CancelToken;
use aidex_drive::DrivePaths;
use aidex_providers::{
    EgressGate, ModelConfig, ModelId, NoVault, OpenAiCompatibleProvider, ProviderConfig,
    ProviderId, ProviderKind, ProviderRegistry,
};
use aidex_sandbox::{
    CommandSpec, ManagedProcess, Mount, NetworkMode, ResourceLimits, Sandbox, SandboxLevel,
    SandboxRequest,
};
use std::{
    net::TcpListener,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::Duration,
};

pub const LOCAL_PROVIDER_ID: &str = "local-llama";
const START_TIMEOUT: Duration = Duration::from_secs(120);
const SETTINGS: &str = "inference-settings.json";
fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|p| p.into_inner())
}
#[derive(Default)]
struct State {
    process: Option<Arc<ManagedProcess>>,
    starting: bool,
    registered: bool,
    options: Option<StartOptions>,
    endpoint: Option<String>,
}

/// One inference server per drive/service. Mutations fail with Conflict while
/// another operation runs; stop cancels active work before waiting for teardown.
pub struct InferenceService {
    paths: DrivePaths,
    registry: Arc<ProviderRegistry>,
    gate: Arc<dyn EgressGate>,
    sandbox: Sandbox,
    operation: Mutex<()>,
    cancel: Mutex<Option<CancelToken>>,
    state: Mutex<State>,
}
struct Operation<'a> {
    service: &'a InferenceService,
    _guard: MutexGuard<'a, ()>,
}
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        *lock(&self.service.cancel) = None;
    }
}
impl InferenceService {
    pub fn open(
        paths: DrivePaths,
        registry: Arc<ProviderRegistry>,
        gate: Arc<dyn EgressGate>,
        sandbox: Sandbox,
    ) -> Result<Self, InferenceError> {
        for dir in [paths.models(), paths.config(), paths.tmp()] {
            paths
                .ensure_dir(&dir)
                .map_err(|e| InferenceError::Io(e.to_string()))?;
        }
        let settings = paths.config().join(SETTINGS);
        if settings.exists() {
            let _: StartOptions = files::read_json(&settings)?;
        }
        Ok(Self {
            paths,
            registry,
            gate,
            sandbox,
            operation: Mutex::new(()),
            cancel: Mutex::new(None),
            state: Mutex::new(State::default()),
        })
    }
    fn begin(&self, cancel: &CancelToken) -> Result<Operation<'_>, InferenceError> {
        let guard = self.operation.try_lock().map_err(|_| {
            InferenceError::Conflict("another inference operation is in progress".into())
        })?;
        files::check_cancel(cancel)?;
        *lock(&self.cancel) = Some(cancel.clone());
        Ok(Operation {
            service: self,
            _guard: guard,
        })
    }
    pub fn import(
        &self,
        source: &Path,
        name: Option<&str>,
        cancel: &CancelToken,
    ) -> Result<ManagedModel, InferenceError> {
        let _operation = self.begin(cancel)?;
        catalog::import(&self.paths, source, name, cancel)
    }
    pub fn catalog(&self) -> Result<Catalog, InferenceError> {
        let path = self.paths.config().join(SETTINGS);
        let settings = if path.exists() {
            Some(files::read_json(&path)?)
        } else {
            None
        };
        Ok(Catalog {
            models: catalog::list(&self.paths)?,
            runtime: runtime::inspect(&self.paths)?.map(|p| p.0),
            settings,
        })
    }
    pub fn remove_model(&self, id: &str) -> Result<(), InferenceError> {
        let _operation = self.begin(&CancelToken::new())?;
        if lock(&self.state)
            .options
            .as_ref()
            .is_some_and(|o| o.model_id == id)
        {
            return Err(InferenceError::Conflict(
                "stop the loaded model before removing it".into(),
            ));
        }
        catalog::remove(&self.paths, id)
    }
    pub fn install_runtime(
        &self,
        source: Option<&Path>,
        cancel: &CancelToken,
    ) -> Result<RuntimeInfo, InferenceError> {
        let _operation = self.begin(cancel)?;
        runtime::install(&self.paths, source, cancel)
    }
    pub fn start(
        &self,
        options: StartOptions,
        cancel: &CancelToken,
    ) -> Result<InferenceStatus, InferenceError> {
        let _operation = self.begin(cancel)?;
        if options.backend == Backend::Gpu {
            return Err(InferenceError::Unavailable(
                "GPU backend cannot be proven available; select auto or CPU".into(),
            ));
        }
        let status = self.status();
        if status.state != "stopped" {
            return Err(InferenceError::Conflict(
                "inference is already running; stop it before restarting".into(),
            ));
        }
        if self
            .registry
            .provider(&ProviderId::new(LOCAL_PROVIDER_ID).unwrap())
            .is_some()
        {
            return Err(InferenceError::Conflict(
                "local-llama provider id is already registered".into(),
            ));
        }
        catalog::validate_id(&options.model_id)?;
        let model = catalog::list(&self.paths)?
            .into_iter()
            .find(|m| m.id == options.model_id)
            .ok_or_else(|| InferenceError::NotFound("model is not installed".into()))?;
        if model.availability != Availability::Available {
            return Err(InferenceError::Integrity(
                "model is missing or its digest changed".into(),
            ));
        }
        let (runtime, manifest) = runtime::inspect(&self.paths)?.ok_or_else(|| {
            InferenceError::NotFound("install the verified llama.cpp runtime first".into())
        })?;
        if !runtime.verified {
            return Err(InferenceError::Integrity(
                "runtime checksum verification failed".into(),
            ));
        }
        let context = options
            .context_window
            .unwrap_or_else(|| model.context_window.unwrap_or(4096).min(4096));
        if !(512..=262144).contains(&context)
            || model.context_window.is_some_and(|limit| context > limit)
        {
            return Err(InferenceError::Invalid(
                "context window must be 512-262144 and within model metadata limit".into(),
            ));
        }
        let threads = options.threads.unwrap_or_else(|| {
            thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(1)
                .min(8)
        });
        if !(1..=256).contains(&threads) {
            return Err(InferenceError::Invalid("threads must be 1-256".into()));
        }
        let configured = options.clone();
        let actual = StartOptions {
            backend: Backend::Cpu,
            context_window: Some(context),
            threads: Some(threads),
            ..options
        };
        {
            let mut state = lock(&self.state);
            state.starting = true;
            state.options = Some(actual.clone());
        }
        let result = self
            .launch(&actual, &manifest, &model.name, cancel)
            .and_then(|status| {
                files::write_json(&self.paths.config().join(SETTINGS), &configured)?;
                files::check_cancel(cancel)?;
                Ok(status)
            });
        if result.is_err() {
            self.clear_process()?;
        }
        result
    }
    fn launch(
        &self,
        options: &StartOptions,
        manifest: &RuntimeManifest,
        model_name: &str,
        cancel: &CancelToken,
    ) -> Result<InferenceStatus, InferenceError> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let runtime_dir = runtime::directory(&self.paths)?;
        let scratch = self
            .paths
            .ensure_dir(&self.paths.tmp().join("local-inference"))
            .map_err(|e| InferenceError::Io(e.to_string()))?;
        let request = SandboxRequest::new(SandboxLevel::Brokered, &runtime_dir, &scratch)
            .with_mount(Mount::read_only(&runtime_dir))
            .with_mount(Mount::read_only(self.paths.models()))
            .with_network(NetworkMode::Normal)
            .with_limits(ResourceLimits {
                max_memory_bytes: Some(16 * 1024 * 1024 * 1024),
                max_output_bytes: 1024 * 1024,
                ..ResourceLimits::default()
            });
        let plan = self
            .sandbox
            .negotiate(&request)
            .map_err(|e| InferenceError::Unavailable(e.to_string()))?;
        let command = CommandSpec::new(runtime_dir.join(&manifest.executable)).args([
            "--model".into(),
            self.paths
                .models()
                .join(&options.model_id)
                .join("model.gguf")
                .to_string_lossy()
                .into_owned(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            port.to_string(),
            "--alias".into(),
            options.model_id.clone(),
            "--ctx-size".into(),
            options.context_window.unwrap().to_string(),
            "--threads".into(),
            options.threads.unwrap().to_string(),
            "--n-gpu-layers".into(),
            "0".into(),
            "--parallel".into(),
            "1".into(),
            "--jinja".into(),
        ]);
        drop(listener);
        files::check_cancel(cancel)?;
        let process = Arc::new(
            self.sandbox
                .spawn(&plan, command)
                .map_err(|e| InferenceError::Unavailable(e.to_string()))?,
        );
        {
            let mut state = lock(&self.state);
            state.process = Some(process.clone());
            state.endpoint = Some(format!("http://127.0.0.1:{port}/v1"));
        }
        crate::readiness::wait(&process, port, cancel, START_TIMEOUT)?;
        files::check_cancel(cancel)?;
        let mut config = ProviderConfig::new(
            ProviderId::new(LOCAL_PROVIDER_ID)
                .map_err(|e| InferenceError::Invalid(e.to_string()))?,
            ProviderKind::OpenaiCompatible,
            format!("http://127.0.0.1:{port}/v1"),
        );
        config.discover_models = false;
        config.models = vec![ModelConfig::new(
            ModelId::new(options.model_id.clone())
                .map_err(|e| InferenceError::Invalid(e.to_string()))?,
            options.context_window.unwrap(),
        )];
        let inner = OpenAiCompatibleProvider::from_config(
            &config,
            Arc::new(NoVault::default()),
            self.gate.clone(),
        )
        .map_err(|e| InferenceError::Invalid(e.to_string()))?;
        let provider =
            crate::provider::LocalProvider::new(inner, &process, &options.model_id, model_name);
        self.registry
            .register(Box::new(provider))
            .map_err(|e| InferenceError::Conflict(e.to_string()))?;
        lock(&self.state).registered = true;
        files::check_cancel(cancel)?;
        lock(&self.state).starting = false;
        Ok(self.status())
    }
    pub fn cancel_operations(&self) {
        if let Some(cancel) = lock(&self.cancel).as_ref() {
            cancel.cancel();
        }
    }
    fn clear_process(&self) -> Result<(), InferenceError> {
        let mut state = lock(&self.state);
        if let Some(process) = state.process.as_ref() {
            process
                .kill_tree()
                .map_err(|e| InferenceError::Unavailable(e.to_string()))?;
            if process
                .wait_timeout(Duration::from_secs(15))
                .map_err(|e| InferenceError::Unavailable(e.to_string()))?
                .is_none()
            {
                return Err(InferenceError::Unavailable(
                    "inference process did not terminate".into(),
                ));
            }
        }
        state.process = None;
        state.starting = false;
        state.options = None;
        state.endpoint = None;
        if state.registered {
            let _ = self
                .registry
                .unregister(&ProviderId::new(LOCAL_PROVIDER_ID).unwrap());
            state.registered = false;
        }
        Ok(())
    }
    pub fn stop(&self) -> Result<InferenceStatus, InferenceError> {
        self.cancel_operations();
        let _guard = lock(&self.operation);
        self.clear_process()?;
        Ok(self.status())
    }
    pub fn status(&self) -> InferenceStatus {
        let mut state = lock(&self.state);
        if !state.starting && state.process.as_ref().is_some_and(|p| !p.is_running()) {
            state.process = None;
            state.options = None;
            state.endpoint = None;
            if state.registered {
                let _ = self
                    .registry
                    .unregister(&ProviderId::new(LOCAL_PROVIDER_ID).unwrap());
                state.registered = false;
            }
        }
        InferenceStatus{state:if state.starting{"starting"}else if state.process.is_some(){"ready"}else{"stopped"}.into(),model_id:state.options.as_ref().map(|o|o.model_id.clone()),backend:state.options.as_ref().map(|o|o.backend),endpoint:state.endpoint.clone(),pid:state.process.as_ref().map(|p|p.pid()),context_window:state.options.as_ref().and_then(|o|o.context_window),threads:state.options.as_ref().and_then(|o|o.threads),controls:state.process.as_ref().map(|p|p.plan().controls.clone()).unwrap_or_default(),limitations:vec!["GPU availability is not proven; auto uses CPU".into(),"Brokered runtime has a killable process tree and cleared environment; filesystem and network are not OS-confined".into(),"The runtime binds IPv4 loopback only; local applications can access its credential-free API".into()]}
    }
}
impl Drop for InferenceService {
    fn drop(&mut self) {
        self.cancel_operations();
        let _ = self.clear_process();
    }
}
