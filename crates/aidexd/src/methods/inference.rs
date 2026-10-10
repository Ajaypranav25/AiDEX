//! Inference DTOs are deliberately owned by `aidex-inference`; IPC only adapts paths/errors.
use crate::{
    errors::{ErrorCode, RpcError},
    router::{Router, RouterError},
    rpc::{expect_no_params, parse_params, to_result},
    services::Services,
};
use aidex_core::CancelToken;
use aidex_inference::{InferenceError, StartOptions};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

pub const INFERENCE_CATALOG: &str = "inference.catalog";
pub const INFERENCE_IMPORT: &str = "inference.import";
pub const INFERENCE_RUNTIME_INSTALL: &str = "inference.runtime.install";
pub const INFERENCE_START: &str = "inference.start";
pub const INFERENCE_STOP: &str = "inference.stop";
pub const INFERENCE_STATUS: &str = "inference.status";
pub const INFERENCE_MODEL_REMOVE: &str = "inference.model.remove";
pub const INFERENCE_CANCEL: &str = "inference.cancel";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImportParams {
    pub source: PathBuf,
    #[serde(default)]
    pub name: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeInstallParams {
    #[serde(default)]
    pub source: Option<PathBuf>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    pub id: String,
}

fn error(error: InferenceError) -> RpcError {
    if error == InferenceError::Cancelled {
        RpcError::new(ErrorCode::Cancelled, error.to_string())
    } else {
        RpcError::service(error.code(), error.to_string())
    }
}

fn with_cancel<T>(
    context: &crate::router::CallContext,
    operation: impl FnOnce(&CancelToken) -> T,
) -> T {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Finished<'a>(&'a AtomicBool);
    impl Drop for Finished<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let token = CancelToken::new();
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Acquire) {
                if context.is_cancelled() {
                    token.cancel();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        let _finished = Finished(&done);
        operation(&token)
    })
}

pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let s = services.clone();
    router.register(INFERENCE_CATALOG, move |_, params| {
        expect_no_params(&params)?;
        to_result(&s.inference().catalog().map_err(error)?)
    })?;
    let s = services.clone();
    router.register(INFERENCE_IMPORT, move |context, params| {
        let p: ImportParams = parse_params(params)?;
        if context.is_cancelled() {
            return Err(RpcError::new(ErrorCode::Cancelled, "request was cancelled"));
        }
        to_result(
            &with_cancel(context, |cancel| {
                s.inference().import(&p.source, p.name.as_deref(), cancel)
            })
            .map_err(error)?,
        )
    })?;
    let s = services.clone();
    router.register(INFERENCE_RUNTIME_INSTALL, move |context, params| {
        let p: RuntimeInstallParams = parse_params(params)?;
        if context.is_cancelled() {
            return Err(RpcError::new(ErrorCode::Cancelled, "request was cancelled"));
        }
        to_result(
            &with_cancel(context, |cancel| {
                s.inference().install_runtime(p.source.as_deref(), cancel)
            })
            .map_err(error)?,
        )
    })?;
    let s = services.clone();
    router.register(INFERENCE_START, move |context, params| {
        let p: StartOptions = parse_params(params)?;
        if context.is_cancelled() {
            return Err(RpcError::new(ErrorCode::Cancelled, "request was cancelled"));
        }
        to_result(&with_cancel(context, |cancel| s.inference().start(p, cancel)).map_err(error)?)
    })?;
    let s = services.clone();
    router.register(INFERENCE_STOP, move |_, params| {
        expect_no_params(&params)?;
        to_result(&s.inference().stop().map_err(error)?)
    })?;
    let s = services.clone();
    router.register(INFERENCE_STATUS, move |_, params| {
        expect_no_params(&params)?;
        to_result(&s.inference().status())
    })?;
    let s = services.clone();
    router.register(INFERENCE_MODEL_REMOVE, move |_, params| {
        let p: RemoveParams = parse_params(params)?;
        s.inference().remove_model(&p.id).map_err(error)?;
        to_result(&serde_json::json!({"removed":true}))
    })?;
    let s = services.clone();
    router.register(INFERENCE_CANCEL, move |_, params| {
        expect_no_params(&params)?;
        s.inference().cancel_operations();
        to_result(&serde_json::json!({"cancelled":true}))
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
    fn cancellation_adapter_tracks_daemon_shutdown_during_work() {
        let shutdown = ShutdownSignal::new();
        let context = CallContext::detached(ClientInfo::new("test", "1"), shutdown.clone());
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            shutdown.request();
        });
        let cancelled = with_cancel(&context, |cancel| {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !cancel.is_cancelled() && std::time::Instant::now() < end {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            cancel.is_cancelled()
        });
        stopper.join().unwrap();
        assert!(cancelled);
    }
    #[test]
    fn rpc_rejects_unknown_fields_and_exposes_real_catalog() {
        let root = crate::services::testing::temp_drive();
        let shutdown = ShutdownSignal::new();
        let services = Arc::new(Services::open(root.path(), shutdown.clone()).unwrap());
        let mut router = Router::new();
        register(&mut router, &services).unwrap();
        let context = CallContext::detached(ClientInfo::new("test", "1"), shutdown);
        let catalog = router
            .call(&context, INFERENCE_CATALOG, serde_json::json!({}))
            .unwrap();
        assert_eq!(catalog["models"], serde_json::json!([]));
        assert!(catalog["runtime"].is_null());
        assert_eq!(
            router
                .call(
                    &context,
                    INFERENCE_IMPORT,
                    serde_json::json!({"source":"tiny.gguf","api_key":"no"})
                )
                .unwrap_err()
                .kind(),
            Some(ErrorCode::InvalidParams)
        );
        assert_eq!(
            router
                .call(
                    &context,
                    INFERENCE_MODEL_REMOVE,
                    serde_json::json!({"id":"../../foo"})
                )
                .unwrap_err()
                .kind(),
            Some(ErrorCode::InvalidArgument)
        );
        assert_eq!(
            router
                .call(&context, INFERENCE_STATUS, serde_json::json!({}))
                .unwrap()["state"],
            "stopped"
        );
    }
}
