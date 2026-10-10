//! Stable, transport-safe service error codes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::EntityId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    InvalidArgument,
    NotFound,
    PermissionDenied,
    Conflict,
    IncompatibleVersion,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceError {
    pub code: ServiceErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<EntityId>,
}
