//! State shared by every command for the life of the app.

use crate::{
    bridge::Bridge,
    lifecycle::{daemon::DaemonSupervisor, drive::DriveSession},
};

/// Managed by Tauri as `Arc<AppState>` so blocking workers can own a handle.
#[derive(Default)]
pub struct AppState {
    pub drive: DriveSession,
    pub bridge: Bridge,
    pub daemon: DaemonSupervisor,
}
