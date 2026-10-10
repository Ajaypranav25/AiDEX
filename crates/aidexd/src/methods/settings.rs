//! Settings stored in the encrypted portable vault.
use crate::{
    errors::{ErrorCode, RpcError},
    router::{Router, RouterError},
    rpc::{expect_no_params, parse_params, to_result},
    services::Services,
};
use aidex_preferences::{PreferencesError, PreferencesService};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetParams {
    patch: serde_json::Value,
}

fn error(error: PreferencesError) -> RpcError {
    match error {
        PreferencesError::Vault(error) => super::vault::vault_error(&error),
        PreferencesError::Invalid(message) => RpcError::invalid_params(message),
        PreferencesError::Unavailable => {
            RpcError::new(ErrorCode::InternalError, "Preference state unavailable")
        }
    }
}
pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError> {
    let settings = Arc::new(PreferencesService::new(services.vault().clone()));
    let shared = settings.clone();
    router.register("settings.get", move |_, params| {
        expect_no_params(&params)?;
        to_result(&shared.get().map_err(error)?)
    })?;
    router.register("settings.set", move |_, params| {
        let params: SetParams = parse_params(params)?;
        to_result(&settings.update(params.patch).map_err(error)?)
    })?;
    Ok(())
}
