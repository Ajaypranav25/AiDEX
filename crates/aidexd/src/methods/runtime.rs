//! Literal runtime configuration RPCs; credentials never enter these DTOs.
use crate::{
    errors::{ErrorCode, RpcError},
    router::{Router, RouterError},
    rpc::{expect_no_params, parse_params, to_result},
    services::Services,
};
use aidex_adapters::{Driver, RuntimeAuth, RuntimeConfig, RuntimeError};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Install {
    driver: Driver,
    #[serde(default)]
    source: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Auth {
    id: String,
    #[serde(default)]
    flow_id: Option<String>,
}
fn error(e: RuntimeError) -> RpcError {
    RpcError::new(
        match e.code.as_str() {
            "vault_locked" => ErrorCode::VaultLocked,
            "invalid_config" | "invalid_request" => ErrorCode::InvalidArgument,
            "not_found" => ErrorCode::NotFound,
            "conflict" | "runtime_busy" => ErrorCode::Conflict,
            "cancelled" => ErrorCode::Cancelled,
            _ => ErrorCode::Unavailable,
        },
        e.message,
    )
    .with_data(json!({"reason":e.code}))
}
fn provider(services: &Services, id: &str) -> Result<String, RpcError> {
    match services.runtime().get(id).map_err(error)?.auth {RuntimeAuth::CodexOauth{provider_id}=>Ok(provider_id),_=>Err(RpcError::new(ErrorCode::Unavailable,"This runtime uses an explicitly scoped vault API key. Browser login is unavailable for this adapter.").with_data(json!({"reason":"auth_unsupported"})))}
}
pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let s = services.clone();
    router.register("runtime.catalog", move |_, p| {
        expect_no_params(&p)?;
        to_result(&s.runtime().catalog())
    })?;
    let s = services.clone();
    router.register("runtime.list", move |_, p| {
        expect_no_params(&p)?;
        to_result(&json!({"runtimes":s.runtime().list().map_err(error)?}))
    })?;
    for method in ["runtime.add", "runtime.update", "runtime.config.save"] {
        let s = services.clone();
        router.register(method, move |_, p| {
            let config: RuntimeConfig = parse_params(p)?;
            if method == "runtime.add"
                && s.runtime()
                    .list()
                    .map_err(error)?
                    .iter()
                    .any(|c| c.id == config.id)
            {
                return Err(RpcError::new(
                    ErrorCode::Conflict,
                    "Runtime ID already exists",
                ));
            }
            if method == "runtime.update" {
                s.runtime().get(&config.id).map_err(error)?;
            }
            let mut domain_error = None;
            let result = s.native().mutate_runtimes(|| {
                let config = s.runtime().save(config).map_err(|e| {
                    domain_error = Some(error(e));
                    "Runtime configuration was refused".to_owned()
                })?;
                let executors = s.runtime().executors().map_err(|e| {
                    domain_error = Some(error(e));
                    "Runtime registry unavailable".to_owned()
                })?;
                Ok((config, executors))
            });
            match result {
                Ok(value) => to_result(&value),
                Err(message) => {
                    Err(domain_error.unwrap_or_else(|| RpcError::new(ErrorCode::Conflict, message)))
                }
            }
        })?;
    }
    let s = services.clone();
    router.register("runtime.remove", move |_, p| {
        let p: Id = parse_params(p)?;
        let mut domain_error = None;
        let removed = s
            .native()
            .mutate_runtimes(|| {
                let removed = s.runtime().remove(&p.id).map_err(|e| {
                    domain_error = Some(error(e));
                    "Runtime removal refused".to_owned()
                })?;
                let executors = s.runtime().executors().map_err(|e| {
                    domain_error = Some(error(e));
                    "Runtime registry unavailable".to_owned()
                })?;
                Ok((removed, executors))
            })
            .map_err(|message| {
                domain_error.unwrap_or_else(|| RpcError::new(ErrorCode::Conflict, message))
            })?;
        Ok(json!({"removed":removed}))
    })?;
    let s = services.clone();
    router.register("runtime.config.get", move |_, p| {
        let p: Id = parse_params(p)?;
        to_result(&s.runtime().get(&p.id).map_err(error)?)
    })?;
    let s = services.clone();
    router.register("runtime.health", move |_, p| {
        let p: Id = parse_params(p)?;
        to_result(&s.runtime().health(&p.id).map_err(error)?)
    })?;
    let s = services.clone();
    router.register("runtime.models", move |_, p| {
        let p: Id = parse_params(p)?;
        Ok(json!({"models":s.runtime().models(&p.id).map_err(error)?}))
    })?;
    let s = services.clone();
    router.register("runtime.install", move |_, p| {
        let p: Install = parse_params(p)?;
        to_result(
            &s.runtime()
                .install(p.driver, p.source.as_deref())
                .map_err(error)?,
        )
    })?;
    let s = services.clone();
    router.register("runtime.auth.start", move |_, p| {
        let p: Id = parse_params(p)?;
        let provider = provider(&s, &p.id)?;
        to_result(
            &s.models()
                .auth()
                .start(&provider, |_, _| Ok(()))
                .map_err(|e| RpcError::new(ErrorCode::Unavailable, e.to_string()))?,
        )
    })?;
    let s = services.clone();
    router.register("runtime.auth.status", move |_, p| {
        let p: Auth = parse_params(p)?;
        let provider = provider(&s, &p.id)?;
        let status = s
            .models()
            .auth()
            .status(p.flow_id.as_deref(), Some(&provider))
            .map_err(|e| RpcError::new(ErrorCode::Unavailable, e.to_string()))?;
        if status.provider_id != provider {
            return Err(RpcError::invalid_params(
                "Flow does not belong to runtime account",
            ));
        }
        to_result(&status)
    })?;
    let s = services.clone();
    router.register("runtime.auth.cancel", move |_, p| {
        let p: Auth = parse_params(p)?;
        let provider = provider(&s, &p.id)?;
        let flow = p
            .flow_id
            .ok_or_else(|| RpcError::invalid_params("flow_id required"))?;
        let status = s
            .models()
            .auth()
            .status(Some(&flow), None)
            .map_err(|e| RpcError::new(ErrorCode::Unavailable, e.to_string()))?;
        if status.provider_id != provider {
            return Err(RpcError::invalid_params(
                "Flow does not belong to runtime account",
            ));
        }
        to_result(
            &s.models()
                .auth()
                .cancel(&flow)
                .map_err(|e| RpcError::new(ErrorCode::Unavailable, e.to_string()))?,
        )
    })?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lifecycle::ShutdownSignal,
        router::{CallContext, ClientInfo},
    };
    #[test]
    fn runtime_rpc_catalog_is_finite_and_config_requires_vault() {
        let drive = crate::services::testing::temp_drive();
        let shutdown = ShutdownSignal::new();
        let services = Arc::new(Services::open(drive.path(), shutdown.clone()).unwrap());
        let context = CallContext::detached(ClientInfo::new("runtime-test", "1"), shutdown);
        let mut router = Router::new();
        register(&mut router, &services).unwrap();
        let catalog = router.call(&context, "runtime.catalog", json!({})).unwrap();
        assert_eq!(catalog.as_array().unwrap().len(), 10);
        assert!(router.call(&context, "runtime.list", json!({})).is_err());
        assert!(
            router
                .call(&context, "runtime.install", json!({"driver":"codex"}))
                .is_err()
        );
        assert!(
            router
                .call(
                    &context,
                    "runtime.add",
                    json!({"id":"a","driver":"codex","executable":"codex.exe","token":"forbidden"})
                )
                .is_err()
        );
    }
}
