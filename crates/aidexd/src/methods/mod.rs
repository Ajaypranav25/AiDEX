//! JSON-RPC adapters, one module per domain.
//!
//! Each `methods/<domain>.rs` exposes
//! `pub(crate) fn register(router: &mut Router, services: &Arc<Services>) -> Result<(), RouterError>`,
//! registers `<domain>.<verb>` methods (and `<domain>.subscribe` streams that
//! emit `<domain>.event` notifications), and keeps domain logic in its crate.
//! Add one `pub mod` line and one `register` line below per domain.

pub mod daemon;
pub mod events;

use crate::{
    router::{Router, RouterError},
    services::Services,
};
use std::sync::Arc;

/// Register every built-in and domain method.
pub(crate) fn register_all(
    router: &mut Router,
    services: &Arc<Services>,
) -> Result<(), RouterError> {
    daemon::register(router, services)?;
    events::register(router, services)?;
    Ok(())
}
