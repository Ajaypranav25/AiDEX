//! Real selected-workspace file, search, Git, environment, and memory methods.
use crate::{
    errors::RpcError,
    methods::native::native_error,
    router::{Router, RouterError},
    rpc::{expect_no_params, parse_params, to_result},
    services::Services,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub path: String,
    #[serde(default)]
    pub start_line: Option<u64>,
    #[serde(default)]
    pub max_lines: Option<u64>,
    #[serde(default)]
    pub max_bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub max_depth: Option<u32>,
    #[serde(default)]
    pub max_entries: Option<u32>,
    #[serde(default)]
    pub include_ignored: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub case_insensitive: bool,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub max_results: Option<u32>,
    #[serde(default)]
    pub context_lines: u32,
    #[serde(default)]
    pub include_ignored: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub max_count: Option<u32>,
    #[serde(default)]
    pub skip: u32,
    #[serde(default)]
    pub paths: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySetParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub key: String,
    pub content: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryDeleteParams {
    #[serde(default)]
    pub workspace_id: Option<String>,
    pub id: String,
}
fn workspace(s: &Services, id: Option<&str>) -> Result<String, RpcError> {
    let id = match id {
        Some(id) => {
            aidex_core::EntityId::new(id).map_err(|e| RpcError::invalid_params(e.to_string()))?
        }
        None => {
            s.workspaces()
                .selected()
                .map_err(|e| native_error(e.to_string()))?
                .ok_or_else(|| native_error("no workspace selected".into()))?
                .id
        }
    };
    s.workspaces()
        .handle(&id)
        .map_err(|e| native_error(e.to_string()))?;
    Ok(id.to_string())
}
fn arguments(p: impl Serialize) -> Result<Value, RpcError> {
    let mut value = serde_json::to_value(p).map_err(|e| native_error(e.to_string()))?;
    if let Some(object) = value.as_object_mut() {
        object.remove("workspace_id");
    }
    Ok(value)
}
pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let s = services.clone();
    router.register("workspace.files.read", move |_, p| {
        let p: ReadParams = parse_params(p)?;
        let id = p.workspace_id.clone();
        to_result(
            &s.native()
                .inspect(id.as_deref(), "fs.read", arguments(p)?)
                .map_err(native_error)?,
        )
    })?;
    let s = services.clone();
    router.register("workspace.files.tree", move |_, p| {
        let p: TreeParams = parse_params(p)?;
        let id = p.workspace_id.clone();
        to_result(
            &s.native()
                .inspect(id.as_deref(), "fs.list", arguments(p)?)
                .map_err(native_error)?,
        )
    })?;
    let s = services.clone();
    router.register("workspace.files.search", move |_, p| {
        let p: SearchParams = parse_params(p)?;
        let id = p.workspace_id.clone();
        to_result(
            &s.native()
                .inspect(id.as_deref(), "fs.search", arguments(p)?)
                .map_err(native_error)?,
        )
    })?;
    let s = services.clone();
    router.register("workspace.git.status", move |_, p| {
        let p: WorkspaceParams = parse_params(p)?;
        to_result(
            &s.native()
                .inspect(p.workspace_id.as_deref(), "git.status", json!({}))
                .map_err(native_error)?,
        )
    })?;
    let s = services.clone();
    router.register("workspace.git.log", move |_, p| {
        let p: LogParams = parse_params(p)?;
        let id = p.workspace_id.clone();
        to_result(
            &s.native()
                .inspect(id.as_deref(), "git.log", arguments(p)?)
                .map_err(native_error)?,
        )
    })?;
    router.register("workspace.environment", move |_, p| {
        expect_no_params(&p)?;
        to_result(&aidex_host::HostProfile::current())
    })?;
    let s = services.clone();
    router.register("native.memory.list", move |_, p| {
        let p: WorkspaceParams = parse_params(p)?;
        let id = workspace(&s, p.workspace_id.as_deref())?;
        to_result(&json!({"memories":s.native().store().memory_list(&id).map_err(native_error)?}))
    })?;
    let s = services.clone();
    router.register("native.memory.set", move |_, p| {
        let p: MemorySetParams = parse_params(p)?;
        let id = workspace(&s, p.workspace_id.as_deref())?;
        to_result(
            &s.native()
                .store()
                .memory_set(&id, &p.key, &p.content)
                .map_err(native_error)?,
        )
    })?;
    let s = services.clone();
    router.register("native.memory.delete", move |_, p| {
        let p: MemoryDeleteParams = parse_params(p)?;
        let id = workspace(&s, p.workspace_id.as_deref())?;
        to_result(
            &json!({"deleted":s.native().store().memory_delete(&id,&p.id).map_err(native_error)?}),
        )
    })?;
    Ok(())
}
