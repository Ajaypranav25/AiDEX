use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Missing,
    Corrupt,
}
impl Availability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Corrupt => "corrupt",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedModel {
    pub id: String,
    pub name: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub gguf_version: u32,
    pub context_window: Option<u32>,
    pub availability: Availability,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub models: Vec<ManagedModel>,
    pub runtime: Option<RuntimeInfo>,
    pub settings: Option<StartOptions>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    #[default]
    Auto,
    Cpu,
    Gpu,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartOptions {
    pub model_id: String,
    #[serde(default)]
    pub backend: Backend,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub threads: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFile {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeManifest {
    pub version: u32,
    pub release: String,
    pub platform: String,
    pub executable: String,
    pub files: Vec<RuntimeFile>,
    #[serde(default)]
    pub gpu_supported: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub release: String,
    pub platform: String,
    pub path: String,
    pub executable: String,
    pub verified: bool,
    pub gpu_supported: bool,
}
/// Host facts local inference plans with (`inference.host`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostReport {
    pub resources: aidex_host::HostResources,
    pub gpu: aidex_host::GpuStatus,
}
#[derive(Clone, Debug, Serialize)]
pub struct InferenceStatus {
    pub state: String,
    pub model_name: Option<String>,
    pub startup_error: Option<String>,
    pub model_id: Option<String>,
    pub backend: Option<Backend>,
    pub endpoint: Option<String>,
    pub pid: Option<u32>,
    pub context_window: Option<u32>,
    pub threads: Option<u32>,
    /// How the running (or starting) model's settings were chosen, with reasons.
    pub plan: Option<crate::LaunchPlan>,
    pub controls: Vec<aidex_sandbox::PlannedControl>,
    pub limitations: Vec<String>,
}
