//! `memory.*` methods: list, add, edit, and remove memories in the user,
//! workspace, or session scope (`aidex-context`).
//!
//! The scope defaults to `workspace` (the named or selected workspace). Memories
//! added or edited through these methods are user-written; agent-written memories
//! are recorded by the harness through the Rust API and marked as such. Secret-like
//! text is refused with `data.reason = "secret_like"`.

use crate::{
    errors::RpcError,
    methods::{context::resolve_workspace, sessions::session_error},
    router::{Router, RouterError},
    rpc::{parse_params, to_result},
    services::Services,
};
use aidex_context::{
    MemoryEdit, MemoryEntry, MemoryError, MemoryScope, MemoryScopeKind, NewMemory,
};
use aidex_core::EntityId;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

pub const LIST: &str = "memory.list";
pub const ADD: &str = "memory.add";
pub const EDIT: &str = "memory.edit";
pub const REMOVE: &str = "memory.remove";

/// Params of `memory.list`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ListParams {
    /// `user`, `workspace` (default), or `session`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScopeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Params of `memory.add`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AddParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScopeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Params of `memory.edit`; omitted fields are unchanged.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EditParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScopeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Params of `memory.remove`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<MemoryScopeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub id: String,
}

/// Result of `memory.list`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryList {
    pub scope: MemoryScope,
    pub entries: Vec<MemoryEntry>,
}

/// Map a memory error to its stable wire code with `data.reason`.
pub fn memory_error(error: MemoryError) -> RpcError {
    let reason = error.reason();
    RpcError::service(error.code(), error.to_string()).with_data(json!({ "reason": reason }))
}

fn exact_id(field: &str, id: &str) -> Result<EntityId, RpcError> {
    EntityId::new(id.trim()).map_err(|error| RpcError::invalid_params(format!("{field}: {error}")))
}

fn scope(
    services: &Services,
    kind: Option<MemoryScopeKind>,
    workspace_id: Option<&str>,
    session_id: Option<&str>,
) -> Result<MemoryScope, RpcError> {
    match kind.unwrap_or(MemoryScopeKind::Workspace) {
        MemoryScopeKind::User => Ok(MemoryScope::User),
        MemoryScopeKind::Workspace => Ok(MemoryScope::Workspace {
            workspace_id: resolve_workspace(services, workspace_id)?.id,
        }),
        MemoryScopeKind::Session => {
            let id = session_id.ok_or_else(|| {
                RpcError::invalid_params("session_id is required for the session scope")
            })?;
            let session = services
                .sessions()
                .get(&exact_id("session_id", id)?)
                .map_err(session_error)?;
            Ok(MemoryScope::Session {
                session_id: session.id,
            })
        }
    }
}

pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let shared = Arc::clone(services);
    router.register(LIST, move |_context, params| {
        let params: ListParams = parse_params(params)?;
        let scope = scope(
            &shared,
            params.scope,
            params.workspace_id.as_deref(),
            params.session_id.as_deref(),
        )?;
        let entries = shared
            .context()
            .memory()
            .list(&scope)
            .map_err(memory_error)?;
        to_result(&MemoryList { scope, entries })
    })?;
    let shared = Arc::clone(services);
    router.register(ADD, move |_context, params| {
        let params: AddParams = parse_params(params)?;
        let scope = scope(
            &shared,
            params.scope,
            params.workspace_id.as_deref(),
            params.session_id.as_deref(),
        )?;
        let entry = shared
            .context()
            .memory()
            .add(&scope, NewMemory::user(params.text).with_tags(params.tags))
            .map_err(memory_error)?;
        to_result(&entry)
    })?;
    let shared = Arc::clone(services);
    router.register(EDIT, move |_context, params| {
        let params: EditParams = parse_params(params)?;
        let scope = scope(
            &shared,
            params.scope,
            params.workspace_id.as_deref(),
            params.session_id.as_deref(),
        )?;
        let edit = MemoryEdit {
            text: params.text,
            tags: params.tags,
            ..MemoryEdit::by_user()
        };
        let entry = shared
            .context()
            .memory()
            .edit(&scope, &exact_id("id", &params.id)?, edit)
            .map_err(memory_error)?;
        to_result(&entry)
    })?;
    let shared = Arc::clone(services);
    router.register(REMOVE, move |_context, params| {
        let params: RemoveParams = parse_params(params)?;
        let scope = scope(
            &shared,
            params.scope,
            params.workspace_id.as_deref(),
            params.session_id.as_deref(),
        )?;
        let entry = shared
            .context()
            .memory()
            .remove(&scope, &exact_id("id", &params.id)?)
            .map_err(memory_error)?;
        to_result(&entry)
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
    use aidex_context::MemorySource;
    use serde_json::Value;

    fn fixture() -> (tempfile::TempDir, Arc<Services>, Router, CallContext) {
        let root = crate::services::testing::temp_drive();
        let shutdown = ShutdownSignal::new();
        let services = Arc::new(Services::open(root.path(), shutdown.clone()).unwrap());
        let workspace = services.workspaces().create_drive("notes").unwrap();
        services.workspaces().select(&workspace.id).unwrap();
        let mut router = Router::new();
        register(&mut router, &services).unwrap();
        let context = CallContext::detached(ClientInfo::new("test", "1"), shutdown);
        (root, services, router, context)
    }

    fn call<T: serde::de::DeserializeOwned>(
        router: &Router,
        context: &CallContext,
        method: &str,
        params: Value,
    ) -> T {
        serde_json::from_value(router.call(context, method, params).unwrap()).unwrap()
    }

    #[test]
    fn memories_round_trip_in_each_scope() {
        let (_root, _services, router, context) = fixture();
        let added: MemoryEntry = call(
            &router,
            &context,
            ADD,
            json!({"text": "use bun", "tags": ["Tools"]}),
        );
        assert_eq!(added.source, MemorySource::User);
        assert_eq!(added.tags, ["tools"]);
        let user: MemoryEntry = call(
            &router,
            &context,
            ADD,
            json!({"scope": "user", "text": "prefer short answers"}),
        );
        let list: MemoryList = call(&router, &context, LIST, json!({}));
        assert!(matches!(list.scope, MemoryScope::Workspace { .. }));
        assert_eq!(list.entries, std::slice::from_ref(&added));
        let edited: MemoryEntry = call(
            &router,
            &context,
            EDIT,
            json!({"id": added.id.as_str(), "text": "use bun 1.3"}),
        );
        assert_eq!(edited.text, "use bun 1.3");
        let removed: MemoryEntry = call(
            &router,
            &context,
            REMOVE,
            json!({"scope": "user", "id": user.id.as_str()}),
        );
        assert_eq!(removed.id, user.id);
        let list: MemoryList = call(&router, &context, LIST, json!({"scope": "user"}));
        assert!(list.entries.is_empty());
    }

    #[test]
    fn secrets_unknown_ids_and_bad_scopes_are_refused() {
        let (_root, _services, router, context) = fixture();
        let token = format!("{}{}", "ghp_", "FAKE0fake1FAKE2fake3FAKE4fake5");
        let error = router
            .call(&context, ADD, json!({"text": format!("token {token}")}))
            .unwrap_err();
        assert_eq!(error.kind(), Some(ErrorCode::InvalidArgument));
        assert_eq!(error.data.unwrap()["reason"], "secret_like");
        let missing = router
            .call(&context, REMOVE, json!({"id": "mem_missing"}))
            .unwrap_err();
        assert_eq!(missing.data.unwrap()["reason"], "memory_not_found");
        let no_session = router
            .call(&context, LIST, json!({"scope": "session"}))
            .unwrap_err();
        assert_eq!(no_session.kind(), Some(ErrorCode::InvalidParams));
        let unknown = router
            .call(
                &context,
                LIST,
                json!({"scope": "session", "session_id": "ses_nope"}),
            )
            .unwrap_err();
        assert_eq!(unknown.kind(), Some(ErrorCode::NotFound));
        let extra = router
            .call(&context, ADD, json!({"text": "x", "source": "agent"}))
            .unwrap_err();
        assert_eq!(extra.kind(), Some(ErrorCode::InvalidParams));
    }
}
