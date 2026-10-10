//! Compose managed inference with the same live registry and offline gate as cloud models.
use super::{ModelServices, ServicesError};
use aidex_drive::DrivePaths;
use aidex_inference::InferenceService;
use aidex_sandbox::Sandbox;
use std::sync::Arc;

pub fn open_inference(
    paths: &DrivePaths,
    models: &ModelServices,
) -> Result<Arc<InferenceService>, ServicesError> {
    InferenceService::open(
        paths.clone(),
        models.registry().clone(),
        models.offline().clone(),
        Sandbox::locate(),
    )
    .map(Arc::new)
    .map_err(|e| ServicesError::new("inference", e.code(), e.to_string()))
}
