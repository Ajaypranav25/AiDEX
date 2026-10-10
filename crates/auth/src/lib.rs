//! Native Sign in with ChatGPT; OAuth secrets stay behind vault handles.
mod oauth;
mod service;
mod storage;
pub use service::{AuthError, AuthService, AuthState, AuthStatus, StartResult};
