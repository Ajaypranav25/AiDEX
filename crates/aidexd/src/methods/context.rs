//! `context.*` and `rules.show` methods over `aidex-context`: background indexing,
//! offline search, debug views of context packs, and project rules.
//!
//! Every method takes an optional `workspace_id` (an id or unique name); without
//! one, the selected workspace is used. Index jobs report `task.*` events (role
//! `context.index`) in the drive log; poll `context.status` for progress. Domain
//! errors carry `data.reason` (see `ContextError::reason`).

use crate::{
    errors::RpcError,
    methods::workspace::workspace_error,
    router::{Router, RouterError},
    rpc::{parse_params, to_result},
    services::Services,
};
use aidex_context::{
    ContextError, ContextPack, DEFAULT_SEARCH_LIMIT, IndexJobStarted, IndexState, IndexStatus,
    IndexTarget, PackInclude, PackRequest, Query, RulesView, SearchHit, SearchMode,
};
use aidex_core::{EntityId, WorkspacePath};
use aidex_workspace::{Workspace, WorkspaceHandle};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

pub const INDEX: &str = "context.index";
pub const INDEX_CANCEL: &str = "context.index.cancel";
pub const STATUS: &str = "context.status";
pub const SEARCH: &str = "context.search";
pub const PACK: &str = "context.pack";
pub const RULES_SHOW: &str = "rules.show";

/// Largest budget `context.pack` accepts, so a pack fits one IPC message.
pub const MAX_RPC_PACK_BUDGET: usize = 100_000;

/// Params of `context.index`, `context.index.cancel`, `context.status`, `rules.show`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceParams {
    /// Workspace id or name; defaults to the selected workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// Params of `context.search`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SearchParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub query: String,
    #[serde(default)]
    pub mode: SearchMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    DEFAULT_SEARCH_LIMIT
}

fn yes() -> bool {
    true
}

/// Params of `context.pack`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub task: String,
    /// 1..=[`MAX_RPC_PACK_BUDGET`] estimated tokens.
    pub budget: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Indexed files to include first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default = "yes")]
    pub rules: bool,
    #[serde(default = "yes")]
    pub memory: bool,
    #[serde(default = "yes")]
    pub search: bool,
}

/// Result of `context.index.cancel`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CancelResult {
    pub workspace_id: EntityId,
    /// Whether a running job was asked to stop.
    pub cancelled: bool,
}

/// Result of `context.search`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    pub workspace_id: EntityId,
    pub index_state: IndexState,
    pub hits: Vec<SearchHit>,
}

/// Result of `rules.show`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RulesResult {
    pub workspace_id: EntityId,
    pub rules: RulesView,
}

/// Map a context error to its stable wire code with `data.reason`.
pub fn context_error(error: ContextError) -> RpcError {
    if let ContextError::Workspace(error) = error {
        return workspace_error(error);
    }
    let reason = error.reason();
    RpcError::service(error.code(), error.to_string()).with_data(json!({ "reason": reason }))
}

/// The named (id or name) or selected workspace.
pub fn resolve_workspace(
    services: &Services,
    reference: Option<&str>,
) -> Result<Workspace, RpcError> {
    match reference
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
    {
        Some(reference) => services
            .workspaces()
            .find(reference)
            .map_err(workspace_error),
        None => services
            .workspaces()
            .selected()
            .map_err(workspace_error)?
            .ok_or_else(|| {
                RpcError::invalid_params("no workspace_id was given and no workspace is selected")
                    .with_data(json!({ "reason": "no_workspace_selected" }))
            }),
    }
}

/// A usable handle (the root must be available) for the named or selected workspace.
pub fn resolve_handle(
    services: &Services,
    reference: Option<&str>,
) -> Result<WorkspaceHandle, RpcError> {
    let workspace = resolve_workspace(services, reference)?;
    services
        .workspaces()
        .handle(&workspace.id)
        .map_err(workspace_error)
}

fn exact_id(field: &str, id: &str) -> Result<EntityId, RpcError> {
    EntityId::new(id.trim()).map_err(|error| RpcError::invalid_params(format!("{field}: {error}")))
}

pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let shared = Arc::clone(services);
    router.register(INDEX, move |_context, params| {
        let params: WorkspaceParams = parse_params(params)?;
        let handle = resolve_handle(&shared, params.workspace_id.as_deref())?;
        let started: IndexJobStarted = shared
            .context()
            .start_index(IndexTarget::from_handle(&handle))
            .map_err(context_error)?;
        to_result(&started)
    })?;
    let shared = Arc::clone(services);
    router.register(INDEX_CANCEL, move |_context, params| {
        let params: WorkspaceParams = parse_params(params)?;
        let workspace = resolve_workspace(&shared, params.workspace_id.as_deref())?;
        let cancelled = shared.context().cancel_index(&workspace.id);
        to_result(&CancelResult {
            workspace_id: workspace.id,
            cancelled,
        })
    })?;
    let shared = Arc::clone(services);
    router.register(STATUS, move |_context, params| {
        let params: WorkspaceParams = parse_params(params)?;
        let workspace = resolve_workspace(&shared, params.workspace_id.as_deref())?;
        let status: IndexStatus = shared
            .context()
            .status(&workspace.id)
            .map_err(context_error)?;
        to_result(&status)
    })?;
    let shared = Arc::clone(services);
    router.register(SEARCH, move |_context, params| {
        let params: SearchParams = parse_params(params)?;
        let workspace = resolve_workspace(&shared, params.workspace_id.as_deref())?;
        let query = Query {
            text: params.query,
            mode: params.mode,
            path_prefix: params.path_prefix,
            limit: params.limit,
        };
        let context = shared.context();
        let hits = context
            .search(&workspace.id, &query)
            .map_err(context_error)?;
        let index_state = context
            .status(&workspace.id)
            .map_err(context_error)?
            .index
            .state;
        to_result(&SearchResult {
            workspace_id: workspace.id,
            index_state,
            hits,
        })
    })?;
    let shared = Arc::clone(services);
    router.register(PACK, move |_context, params| {
        let params: PackParams = parse_params(params)?;
        if params.budget == 0 || params.budget > MAX_RPC_PACK_BUDGET {
            return Err(RpcError::invalid_params(format!(
                "budget must be between 1 and {MAX_RPC_PACK_BUDGET}"
            )));
        }
        let handle = resolve_handle(&shared, params.workspace_id.as_deref())?;
        let session_id = params
            .session_id
            .as_deref()
            .map(|id| exact_id("session_id", id))
            .transpose()?;
        if let Some(session_id) = &session_id {
            shared
                .sessions()
                .get(session_id)
                .map_err(crate::methods::sessions::session_error)?;
        }
        let explicit_paths = params
            .paths
            .iter()
            .map(|path| {
                WorkspacePath::new(path.trim())
                    .map_err(|error| RpcError::invalid_params(format!("paths: `{path}`: {error}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut request = PackRequest::new(handle.id().clone(), params.task, params.budget)
            .with_include(PackInclude {
                rules: params.rules,
                memory: params.memory,
                search_hits: params.search,
                explicit_paths,
            });
        request.session_id = session_id;
        let pack: ContextPack = shared
            .context()
            .assemble(&IndexTarget::from_handle(&handle), &request)
            .map_err(context_error)?;
        to_result(&pack)
    })?;
    let shared = Arc::clone(services);
    router.register(RULES_SHOW, move |_context, params| {
        let params: WorkspaceParams = parse_params(params)?;
        let handle = resolve_handle(&shared, params.workspace_id.as_deref())?;
        let rules = shared
            .context()
            .rules(handle.root())
            .map_err(context_error)?;
        to_result(&RulesResult {
            workspace_id: handle.id().clone(),
            rules: rules.view(),
        })
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        errors::ErrorCode,
        lifecycle::ShutdownSignal,
        router::{CallContext, ClientInfo},
    };
    use aidex_context::{IndexJobStatus, JobState};
    use serde_json::Value;
    use std::time::{Duration, Instant};

    struct Fixture {
        _root: tempfile::TempDir,
        project: tempfile::TempDir,
        services: Arc<Services>,
        router: Router,
        context: CallContext,
        workspace: EntityId,
    }

    fn fixture() -> Fixture {
        let root = crate::services::testing::temp_drive();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("lib.rs"), "pub fn parse_config() {}\n").unwrap();
        std::fs::create_dir_all(project.path().join(".aidex")).unwrap();
        std::fs::write(
            project.path().join(".aidex").join("rules.toml"),
            "schema_version = 1\n[project]\ntests = \"cargo test\"\n[agent]\nrequire_approval = [\"git.push\"]\n",
        )
        .unwrap();
        let shutdown = ShutdownSignal::new();
        let services = Arc::new(Services::open(root.path(), shutdown.clone()).unwrap());
        let workspace = services
            .workspaces()
            .add_host(project.path(), Some("proj"))
            .unwrap()
            .id;
        let mut router = Router::new();
        register(&mut router, &services).unwrap();
        let context = CallContext::detached(ClientInfo::new("test", "1"), shutdown);
        Fixture {
            _root: root,
            project,
            services,
            router,
            context,
            workspace,
        }
    }

    fn call<T: serde::de::DeserializeOwned>(fixture: &Fixture, method: &str, params: Value) -> T {
        serde_json::from_value(
            fixture
                .router
                .call(&fixture.context, method, params)
                .unwrap(),
        )
        .unwrap()
    }

    fn wait_done(fixture: &Fixture) -> IndexJobStatus {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let status: IndexStatus = call(fixture, STATUS, json!({"workspace_id": "proj"}));
            let job = status.job.unwrap();
            if job.state != JobState::Running || Instant::now() > deadline {
                return job;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn index_search_pack_and_rules_round_trip() {
        let fixture = fixture();
        let started: IndexJobStarted = call(&fixture, INDEX, json!({"workspace_id": "proj"}));
        assert_eq!(started.workspace_id, fixture.workspace);
        let job = wait_done(&fixture);
        assert_eq!(job.state, JobState::Completed, "{job:?}");
        assert_eq!(job.report.unwrap().added, 1);

        let result: SearchResult = call(
            &fixture,
            SEARCH,
            json!({"workspace_id": fixture.workspace.as_str(), "query": "parse_config", "mode": "symbol"}),
        );
        assert_eq!(result.index_state, IndexState::Complete);
        assert_eq!(result.hits[0].path, "lib.rs");

        let pack: ContextPack = call(
            &fixture,
            PACK,
            json!({"workspace_id": "proj", "task": "parse_config", "budget": 2000}),
        );
        assert!(pack.total_tokens <= 2000);
        assert!(pack.render().contains("pub fn parse_config"));

        let rules: RulesResult = call(&fixture, RULES_SHOW, json!({"workspace_id": "proj"}));
        assert_eq!(rules.rules.project.tests.as_deref(), Some("cargo test"));
        assert_eq!(rules.rules.require_approval, ["git.push"]);

        let cancel: CancelResult = call(&fixture, INDEX_CANCEL, json!({"workspace_id": "proj"}));
        assert!(!cancel.cancelled);
        // Events: task.created/started/completed in the drive log.
        let page = fixture
            .services
            .events()
            .replay(
                &aidex_events::EventFilter::all(),
                aidex_events::Cursor::ZERO,
                100,
            )
            .unwrap();
        let names: Vec<&str> = page.events.iter().map(|event| event.event.name()).collect();
        assert!(names.contains(&"task.completed"), "{names:?}");
        drop(fixture.project);
    }

    #[test]
    fn params_are_closed_and_workspaces_must_resolve() {
        let fixture = fixture();
        for (method, params) in [
            (SEARCH, json!({"query": "x", "extra": 1})),
            (
                SEARCH,
                json!({"workspace_id": "proj", "query": "x", "mode": "fuzzy"}),
            ),
            (
                PACK,
                json!({"workspace_id": "proj", "task": "x", "budget": 0}),
            ),
            (
                PACK,
                json!({"workspace_id": "proj", "task": "x", "budget": 1_000_000}),
            ),
            (
                PACK,
                json!({"workspace_id": "proj", "task": "x", "budget": 10, "paths": ["../x"]}),
            ),
            (STATUS, json!({"workspace": "proj"})),
        ] {
            let error = fixture
                .router
                .call(&fixture.context, method, params.clone())
                .unwrap_err();
            assert_eq!(
                error.kind(),
                Some(ErrorCode::InvalidParams),
                "{method} {params}"
            );
        }
        let none = fixture
            .router
            .call(&fixture.context, STATUS, json!({}))
            .unwrap_err();
        assert_eq!(none.data.unwrap()["reason"], "no_workspace_selected");
        fixture
            .services
            .workspaces()
            .select(&fixture.workspace)
            .unwrap();
        let status: IndexStatus = call(&fixture, STATUS, json!({}));
        assert_eq!(status.index.state, IndexState::Empty);
        let missing = fixture
            .router
            .call(
                &fixture.context,
                SEARCH,
                json!({"workspace_id": "nope", "query": "x"}),
            )
            .unwrap_err();
        assert_eq!(missing.kind(), Some(ErrorCode::NotFound));
        let empty = fixture
            .router
            .call(&fixture.context, SEARCH, json!({"query": "  "}))
            .unwrap_err();
        assert_eq!(empty.data.unwrap()["reason"], "invalid_query");
        let session = fixture
            .router
            .call(
                &fixture.context,
                PACK,
                json!({"task": "x", "budget": 10, "session_id": "ses_missing"}),
            )
            .unwrap_err();
        assert_eq!(session.kind(), Some(ErrorCode::NotFound));
    }
}
