//! Repository context commands over the daemon's `context.*`, `memory.*`, and
//! `rules.show` methods: `aidex index`, `aidex search`, `aidex context pack`,
//! `aidex memory`, and `aidex rules`. Each takes `--workspace <ID|NAME>`
//! (default: the selected workspace) and `--drive`.

pub mod index;
pub mod memory;
pub mod pack;
mod render;
pub mod rules;
pub mod search;

use crate::{commands::daemon::connect, exit::CliError};
use serde_json::{Value, json};
use std::path::Path;

/// `{"workspace_id": ...}` when a workspace was named, else `{}`.
fn workspace_params(workspace: Option<&str>) -> Value {
    match workspace {
        Some(workspace) => json!({ "workspace_id": workspace }),
        None => json!({}),
    }
}

fn call<T: serde::de::DeserializeOwned>(
    drive: Option<&Path>,
    method: &str,
    params: Value,
) -> Result<T, CliError> {
    Ok(connect(drive)?.call_as(method, params)?)
}
