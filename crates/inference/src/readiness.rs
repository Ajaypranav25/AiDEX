use crate::{InferenceError, files};
use aidex_core::CancelToken;
use aidex_sandbox::ManagedProcess;
use std::{
    thread,
    time::{Duration, Instant},
};
pub(crate) fn wait(
    process: &ManagedProcess,
    port: u16,
    cancel: &CancelToken,
    timeout: Duration,
) -> Result<(), InferenceError> {
    let config = ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_millis(500)))
        .build();
    let agent: ureq::Agent = config.into();
    let deadline = Instant::now() + timeout;
    loop {
        files::check_cancel(cancel)?;
        if !process.is_running() {
            return Err(InferenceError::Unavailable(
                "llama-server exited before becoming healthy".into(),
            ));
        }
        if Instant::now() >= deadline {
            return Err(InferenceError::Unavailable(
                "llama-server readiness deadline exceeded".into(),
            ));
        }
        if healthy(&agent, port) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn healthy(agent: &ureq::Agent, port: u16) -> bool {
    let Ok(mut response) = agent.get(format!("http://127.0.0.1:{port}/health")).call() else {
        return false;
    };
    if response.status() != 200 {
        return false;
    }
    let Ok(body) = response
        .body_mut()
        .with_config()
        .limit(4096)
        .read_to_string()
    else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .is_some_and(|v| v["status"] == "ok")
}
