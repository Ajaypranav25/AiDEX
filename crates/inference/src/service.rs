//! [`LocalInference`]: the local model service the daemon composes (spec §96).
//! It owns the registry, the runtime catalog, the resident servers, and the
//! idle reaper, and emits `model.loaded` / `model.unloaded` / `model.error`.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    thread,
    time::Duration,
};

use aidex_core::{CancelReason, CancelToken, EntityId};
use aidex_drive::DrivePaths;
use aidex_events::{EventKind, EventScope, EventSink};
use aidex_host::{GpuProbeOptions, GpuStatus, HostResources, probe_gpus};
use aidex_providers::{ChatRequest, ChatResponse, FinishReason, ProviderId, StreamEvent};
use aidex_sandbox::Sandbox;
use serde::{Deserialize, Serialize};

use crate::{
    ComposeRequest, HostSnapshot, ImportOptions, ImportProgress, InferenceError, LaunchPlan,
    LocalModelId, ModelRegistry, RegistryEntry, RegistryListing, RemovedModel, Result, Verified,
    compose,
    import::import_model_with,
    runtime::{
        CatalogEntry, InstallPhase, InstalledListing, InstalledRuntime, RuntimeCatalog,
        install_from_catalog, list_installed,
    },
    scheduler::{Admission, Lease, LoadedModel, LoadedSummary, Residency, SchedulerPolicy, Slot},
    server::{ManagedServer, ServerConfig, ServerInfo, ServerLaunch},
    verify::{VerificationCache, verify_model_with},
};

/// The provider id local models are served under (`local/<model>`).
pub const LOCAL_PROVIDER_ID: &str = "local";
/// Server logs directory under `DrivePaths::logs()`.
pub const LOGS_DIR: &str = "inference";
/// Scratch directory under `DrivePaths::tmp()`.
pub const SCRATCH_DIR: &str = "inference";
/// How long a waiter sleeps between residency checks.
const WAIT_STEP: Duration = Duration::from_millis(100);

/// Service settings.
#[derive(Clone, Debug)]
pub struct InferenceOptions {
    /// Unload a model unused this long (`None` keeps models until unloaded).
    pub idle_timeout: Option<Duration>,
    /// How often the idle reaper runs.
    pub reaper_interval: Duration,
    pub scheduler: SchedulerPolicy,
    pub server: ServerConfig,
    /// Fixed host memory/CPU facts instead of reading them for each plan
    /// (deterministic tests and diagnostics); `None` reads the live host.
    pub resources: Option<HostResources>,
}

impl Default for InferenceOptions {
    fn default() -> Self {
        Self {
            idle_timeout: Some(Duration::from_secs(10 * 60)),
            reaper_interval: Duration::from_secs(5),
            scheduler: SchedulerPolicy::default(),
            server: ServerConfig::default(),
            resources: None,
        }
    }
}

/// Runtimes for this host: what the catalog pins and what the drive has.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeInventory {
    /// This host's runtime platform (`None` when unsupported).
    pub platform: Option<String>,
    /// Pinned archives for this platform.
    pub catalog: Vec<CatalogEntry>,
    /// Installs on the drive (every platform).
    pub installed: InstalledListing,
}

/// A launch plan with the host facts it was computed from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanReport {
    pub plan: LaunchPlan,
    pub host: HostSnapshot,
}

/// The outcome of `load`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadReport {
    pub model: LocalModelId,
    /// The model was already resident; its existing plan is reported.
    pub already_loaded: bool,
    pub plan: LaunchPlan,
    pub server: ServerInfo,
    /// Present when this call verified the model file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<Verified>,
}

/// Load-time choices.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadOptions {
    #[serde(default)]
    pub compose: ComposeRequest,
    /// Run the opt-in GPU probe before planning (the result is reused afterwards).
    #[serde(default)]
    pub probe_gpu: bool,
}

pub(crate) struct Inner {
    paths: DrivePaths,
    registry: ModelRegistry,
    catalog: RuntimeCatalog,
    sandbox: Sandbox,
    events: Arc<dyn EventSink>,
    options: InferenceOptions,
    residency: Arc<Residency>,
    gpu: Mutex<GpuStatus>,
    import_lock: Mutex<()>,
    install_lock: Mutex<()>,
    provider_id: ProviderId,
    provider_entity: EntityId,
}

/// The local model service. Cloning shares it.
#[derive(Clone)]
pub struct LocalInference {
    pub(crate) inner: Arc<Inner>,
}

impl std::fmt::Debug for LocalInference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalInference")
            .field("drive", &self.inner.paths.root())
            .finish_non_exhaustive()
    }
}

impl LocalInference {
    /// Open the service on a drive with the built-in runtime catalog.
    pub fn open(
        paths: &DrivePaths,
        sandbox: Sandbox,
        events: Arc<dyn EventSink>,
        options: InferenceOptions,
    ) -> Result<Self> {
        Self::with_catalog(paths, sandbox, events, options, RuntimeCatalog::builtin()?)
    }

    /// Open with an explicit catalog (tests and embedders).
    pub fn with_catalog(
        paths: &DrivePaths,
        sandbox: Sandbox,
        events: Arc<dyn EventSink>,
        options: InferenceOptions,
        catalog: RuntimeCatalog,
    ) -> Result<Self> {
        let registry = ModelRegistry::open(paths)?;
        let provider_id = ProviderId::new(LOCAL_PROVIDER_ID)
            .map_err(|error| InferenceError::InvalidArgument(error.to_string()))?;
        let provider_entity = provider_id.as_entity_id().clone();
        let inner = Arc::new(Inner {
            paths: paths.clone(),
            registry,
            catalog,
            sandbox,
            events,
            options,
            residency: Arc::new(Residency::default()),
            gpu: Mutex::new(GpuStatus::NotProbed),
            import_lock: Mutex::new(()),
            install_lock: Mutex::new(()),
            provider_id,
            provider_entity,
        });
        if let Some(timeout) = inner.options.idle_timeout {
            start_reaper(
                Arc::downgrade(&inner),
                timeout,
                inner.options.reaper_interval,
            )?;
        }
        Ok(Self { inner })
    }

    pub fn paths(&self) -> &DrivePaths {
        &self.inner.paths
    }

    pub fn registry(&self) -> &ModelRegistry {
        &self.inner.registry
    }

    pub fn catalog(&self) -> &RuntimeCatalog {
        &self.inner.catalog
    }

    pub fn options(&self) -> &InferenceOptions {
        &self.inner.options
    }

    /// `local`.
    pub fn provider_id(&self) -> &ProviderId {
        &self.inner.provider_id
    }

    /// The `ModelProvider` serving registered models as `local/<id>`.
    pub fn provider(&self) -> crate::LocalLlamaProvider {
        crate::LocalLlamaProvider::new(self.clone())
    }

    pub fn models(&self) -> Result<RegistryListing> {
        self.inner.registry.list()
    }

    pub fn import(
        &self,
        source: &Path,
        options: &ImportOptions,
        progress: &mut dyn FnMut(&ImportProgress),
        cancel: &CancelToken,
    ) -> Result<RegistryEntry> {
        let _guard = self
            .inner
            .import_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        import_model_with(&self.inner.paths, source, options, progress, cancel)
    }

    /// Verify a model; `force` rehashes even when a cached result applies.
    pub fn verify(
        &self,
        id: &LocalModelId,
        force: bool,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<Verified> {
        let entry = self.inner.registry.get(id)?;
        verify_model_with(&self.inner.paths, &entry, force, progress, cancel)
    }

    /// Unload (if resident) and delete the registry entry and imported files.
    pub fn remove(&self, id: &LocalModelId) -> Result<RemovedModel> {
        let entry = self.inner.registry.get(id)?;
        self.unload(id);
        let removed = self.inner.registry.remove(id)?;
        if let Ok(cache) = VerificationCache::open(&self.inner.paths) {
            let _ = cache.forget(&entry.file);
        }
        Ok(removed)
    }

    pub fn runtimes(&self) -> Result<RuntimeInventory> {
        let platform = aidex_drive::platform_runtime_name().map(str::to_owned);
        let catalog = platform
            .as_deref()
            .map(|platform| self.inner.catalog.for_platform(platform).cloned().collect())
            .unwrap_or_default();
        Ok(RuntimeInventory {
            platform,
            catalog,
            installed: list_installed(&self.inner.paths)?,
        })
    }

    /// Install a pinned runtime archive (matched to the catalog by SHA-256).
    pub fn install_runtime(
        &self,
        archive: &Path,
        progress: &mut dyn FnMut(InstallPhase),
        cancel: &CancelToken,
    ) -> Result<InstalledRuntime> {
        let _guard = self
            .inner
            .install_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        install_from_catalog(
            &self.inner.paths,
            archive,
            &self.inner.catalog,
            progress,
            cancel,
        )
    }

    /// Host facts; runs the opt-in GPU probe once when `probe_gpu` is set.
    pub fn host(&self, probe_gpu: bool) -> HostSnapshot {
        let gpu = {
            let mut cached = self
                .inner
                .gpu
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if probe_gpu && *cached == GpuStatus::NotProbed {
                *cached = probe_gpus(&GpuProbeOptions::default());
            }
            cached.clone()
        };
        let mut host = HostSnapshot::current(gpu);
        if let Some(resources) = &self.inner.options.resources {
            host.resources = resources.clone();
        }
        host
    }

    /// Explain how `id` would be launched on this host now.
    pub fn plan(&self, id: &LocalModelId, options: &LoadOptions) -> Result<PlanReport> {
        let entry = self.inner.registry.get(id)?;
        let host = self.host(options.probe_gpu);
        let plan = self.compose_for(&entry, &host, &options.compose)?.0;
        Ok(PlanReport { plan, host })
    }

    fn compose_for(
        &self,
        entry: &RegistryEntry,
        host: &HostSnapshot,
        request: &ComposeRequest,
    ) -> Result<(LaunchPlan, InstalledRuntime)> {
        let installed = list_installed(&self.inner.paths)?.runtimes;
        let plan = compose(entry, host, &installed, request).map_err(|error| match error {
            InferenceError::RuntimeNotInstalled { platform, reason } => {
                let pinned: Vec<&str> = self
                    .inner
                    .catalog
                    .for_platform(&platform)
                    .map(|entry| entry.archive.as_str())
                    .collect();
                let reason = if pinned.is_empty() {
                    format!("{reason}; the catalog pins no runtime for {platform}")
                } else {
                    format!("{reason}; pinned archives: {}", pinned.join(", "))
                };
                InferenceError::RuntimeNotInstalled { platform, reason }
            }
            other => other,
        })?;
        let runtime = installed
            .into_iter()
            .find(|runtime| runtime.relative_dir == plan.runtime.dir)
            .ok_or_else(|| InferenceError::RuntimeNotInstalled {
                platform: plan.runtime.platform.clone(),
                reason: "the planned runtime disappeared".into(),
            })?;
        Ok((plan, runtime))
    }

    /// Make `id` resident (verifying it and its runtime first).
    pub fn load(
        &self,
        id: &LocalModelId,
        options: &LoadOptions,
        cancel: &CancelToken,
    ) -> Result<LoadReport> {
        let (lease, verification) = self.acquire(id, options, cancel)?;
        let model = lease.model();
        Ok(LoadReport {
            model: id.clone(),
            already_loaded: verification.is_none(),
            plan: model.plan.clone(),
            server: model.server.info(),
            verification,
        })
    }

    /// Stop `id`'s server. Returns whether it was resident.
    pub fn unload(&self, id: &LocalModelId) -> bool {
        let model = {
            let mut resident = self.inner.residency.lock();
            resident.take_ready(id)
        };
        self.inner.residency.notify();
        match model {
            Some(model) => {
                self.inner.stop_model(&model);
                true
            }
            None => false,
        }
    }

    /// Resident models.
    pub fn loaded(&self) -> Vec<LoadedSummary> {
        let resident = self.inner.residency.lock();
        resident
            .models
            .values()
            .filter_map(|slot| match slot {
                Slot::Ready(model) => Some(model.summary()),
                Slot::Loading { .. } => None,
            })
            .collect()
    }

    /// Context window the provider advertises for `entry`: the running plan's,
    /// else the recommended context.
    pub(crate) fn context_window(&self, entry: &RegistryEntry) -> u32 {
        let resident = self.inner.residency.lock();
        match resident.models.get(&entry.id) {
            Some(Slot::Ready(model)) => model.plan.ctx_size,
            _ => entry.memory.recommended_context,
        }
    }

    /// Chat with `id`, loading it on first use.
    pub fn chat(
        &self,
        id: &LocalModelId,
        request: &ChatRequest,
        on_event: &mut dyn FnMut(&StreamEvent),
        cancel: &CancelToken,
    ) -> Result<ChatResponse> {
        let lease = match self.acquire(id, &LoadOptions::default(), cancel) {
            Ok((lease, _)) => lease,
            Err(error) => {
                if error == InferenceError::Cancelled {
                    // The client reports this itself once a request is in flight.
                    on_event(&StreamEvent::Done {
                        finish_reason: FinishReason::Cancelled,
                    });
                }
                return Err(error);
            }
        };
        lease
            .model()
            .server
            .chat(request, on_event, cancel)
            .map_err(InferenceError::Provider)
    }

    /// Stop every resident server and refuse new loads.
    pub fn shutdown(&self) {
        let models: Vec<Arc<LoadedModel>> = {
            let mut resident = self.inner.residency.lock();
            resident.closed = true;
            let ids: Vec<LocalModelId> = resident.models.keys().cloned().collect();
            ids.iter()
                .filter_map(|id| resident.take_ready(id))
                .collect()
        };
        self.inner.residency.notify();
        for model in models {
            self.inner.stop_model(&model);
        }
    }

    /// A lease on a resident `id`, loading it if needed. The second value is
    /// the verification performed when this call loaded the model.
    pub(crate) fn acquire(
        &self,
        id: &LocalModelId,
        options: &LoadOptions,
        cancel: &CancelToken,
    ) -> Result<(Lease, Option<Verified>)> {
        let entry = self.inner.registry.get(id)?;
        let large = entry.size_bytes >= self.inner.options.scheduler.large_model_bytes;
        let policy = self.inner.options.scheduler;
        let mut resident = self.inner.residency.lock();
        loop {
            if resident.closed {
                return Err(InferenceError::Busy {
                    reason: "local inference is shutting down".into(),
                });
            }
            match resident.admit(id, large, &policy) {
                Admission::Ready(model) => {
                    return Ok((Lease::new(model, Arc::clone(&self.inner.residency)), None));
                }
                Admission::Wait(reason) => {
                    match cancel.state() {
                        Some(CancelReason::Cancelled) => return Err(InferenceError::Cancelled),
                        Some(CancelReason::DeadlineExceeded) => {
                            return Err(InferenceError::Busy { reason });
                        }
                        None => {}
                    }
                    resident = self.inner.residency.wait(resident, WAIT_STEP);
                }
                Admission::Load { evict } => {
                    let evicted: Vec<Arc<LoadedModel>> = evict
                        .iter()
                        .filter_map(|other| {
                            let model = resident.take_ready(other);
                            resident.models.remove(other);
                            model
                        })
                        .collect();
                    resident.models.insert(id.clone(), Slot::Loading { large });
                    drop(resident);
                    for model in evicted {
                        self.inner.stop_model(&model);
                    }
                    let started = self.start_model(&entry, large, options, cancel);
                    let mut resident = self.inner.residency.lock();
                    resident.models.remove(id);
                    let mut discard = None;
                    let outcome = match started {
                        Ok((model, verified)) if !resident.closed => {
                            resident
                                .models
                                .insert(id.clone(), Slot::Ready(Arc::clone(&model)));
                            let lease =
                                Lease::new(Arc::clone(&model), Arc::clone(&self.inner.residency));
                            self.inner.emit(EventKind::ModelLoaded {
                                provider_id: self.inner.provider_entity.clone(),
                                model_id: id.entity_id().clone(),
                            });
                            Ok((lease, Some(verified)))
                        }
                        Ok((model, _)) => {
                            discard = Some(model);
                            Err(InferenceError::Busy {
                                reason: "local inference is shutting down".into(),
                            })
                        }
                        Err(error) => {
                            if error != InferenceError::Cancelled {
                                self.inner.emit(EventKind::ModelError {
                                    provider_id: self.inner.provider_entity.clone(),
                                    model_id: id.entity_id().clone(),
                                    code: error.kind().into(),
                                    message: error.to_string(),
                                });
                            }
                            Err(error)
                        }
                    };
                    drop(resident);
                    self.inner.residency.notify();
                    if let Some(model) = discard {
                        model.server.stop();
                    }
                    return outcome;
                }
            }
        }
    }

    fn start_model(
        &self,
        entry: &RegistryEntry,
        large: bool,
        options: &LoadOptions,
        cancel: &CancelToken,
    ) -> Result<(Arc<LoadedModel>, Verified)> {
        let paths = &self.inner.paths;
        let verified = verify_model_with(paths, entry, false, &mut |_| {}, cancel)?;
        let host = self.host(options.probe_gpu);
        let (plan, runtime) = self.compose_for(entry, &host, &options.compose)?;
        runtime.verify(&self.inner.catalog, cancel)?;
        let model_path = entry.model_path(paths)?;
        let logs_dir = paths.ensure_dir(&paths.logs().join(LOGS_DIR))?;
        let scratch_root = paths.ensure_dir(&paths.tmp().join(SCRATCH_DIR))?;
        let launch = ServerLaunch {
            runtime: &runtime,
            model_id: &entry.id,
            model_path: &model_path,
            plan: &plan,
            logs_dir: &logs_dir,
            scratch_root: &scratch_root,
        };
        let server = ManagedServer::start(
            &self.inner.sandbox,
            &launch,
            &self.inner.options.server,
            cancel,
        )?;
        Ok((
            Arc::new(LoadedModel::new(entry.id.clone(), server, plan, large)),
            verified,
        ))
    }

    /// Logs directory of managed servers.
    pub fn logs_dir(&self) -> PathBuf {
        self.inner.paths.logs().join(LOGS_DIR)
    }
}

impl Inner {
    fn emit(&self, event: EventKind) {
        // Best effort: an event failure never fails the model operation.
        let _ = self.events.emit(EventScope::global(), event);
    }

    fn stop_model(&self, model: &LoadedModel) {
        model.server.stop();
        self.emit(EventKind::ModelUnloaded {
            provider_id: self.provider_entity.clone(),
            model_id: model.id.entity_id().clone(),
        });
    }

    fn reap(&self, timeout: Duration) {
        let idle = self.residency.lock().take_idle(timeout);
        if !idle.is_empty() {
            self.residency.notify();
        }
        for model in idle {
            self.stop_model(&model);
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let models: Vec<Arc<LoadedModel>> = {
            let mut resident = self.residency.lock();
            resident.closed = true;
            let ids: Vec<LocalModelId> = resident.models.keys().cloned().collect();
            ids.iter()
                .filter_map(|id| resident.take_ready(id))
                .collect()
        };
        for model in models {
            self.stop_model(&model);
        }
    }
}

fn start_reaper(inner: Weak<Inner>, timeout: Duration, interval: Duration) -> Result<()> {
    thread::Builder::new()
        .name("aidex-inference-reaper".into())
        .spawn(move || {
            loop {
                thread::sleep(interval);
                let Some(inner) = inner.upgrade() else {
                    return;
                };
                if inner.residency.lock().closed {
                    return;
                }
                inner.reap(timeout);
            }
        })
        .map(drop)
        .map_err(|error| InferenceError::io("cannot start the idle reaper", &error))
}
