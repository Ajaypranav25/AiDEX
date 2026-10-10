//! Loopback HTTP calls AiDEX makes to its own llama-server besides chat:
//! the `/health` readiness probe and a timed completion for benchmarks.

use std::{io::Read, time::Duration};

use aidex_providers::SecretBytes;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::{InferenceError, Result};

/// Largest benchmark response read.
const MAX_BODY: u64 = 1024 * 1024;

/// Token counts and rates of one completion, as llama-server measured them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionTimings {
    pub prompt_tokens: u64,
    pub prompt_ms: f64,
    pub prompt_tokens_per_second: f64,
    pub generated_tokens: u64,
    pub generation_ms: f64,
    pub generation_tokens_per_second: f64,
}

pub(crate) struct HealthClient {
    base: String,
    probe: ureq::Agent,
    completion: ureq::Agent,
}

fn agent(connect: Duration, total: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .timeout_connect(Some(connect))
        .timeout_global(Some(total))
        .build()
        .into()
}

impl HealthClient {
    pub(crate) fn new(base: &str) -> Self {
        Self {
            base: base.to_owned(),
            probe: agent(Duration::from_millis(500), Duration::from_secs(2)),
            completion: agent(Duration::from_secs(2), Duration::from_secs(600)),
        }
    }

    /// `GET /health` answered 200 (503 while the model loads).
    pub(crate) fn is_ready(&self) -> bool {
        match self.probe.get(format!("{}/health", self.base)).call() {
            Ok(mut response) => {
                let ready = response.status().as_u16() == 200;
                let _ = response
                    .body_mut()
                    .as_reader()
                    .take(4096)
                    .read_to_end(&mut Vec::new());
                ready
            }
            Err(_) => false,
        }
    }

    pub(crate) fn timed_completion(
        &self,
        key: &SecretBytes,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<CompletionTimings> {
        let failure = |reason: String| {
            InferenceError::Provider(aidex_providers::ProviderError::unavailable(reason))
        };
        let token = std::str::from_utf8(key.expose())
            .map_err(|_| failure("the server key is not text".into()))?;
        let authorization = Zeroizing::new(format!("Bearer {token}"));
        let body = json!({
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
            "temperature": 0,
            "stream": false,
            "cache_prompt": false,
        });
        let mut response = self
            .completion
            .post(format!("{}/v1/chat/completions", self.base))
            .header("Authorization", authorization.as_str())
            .content_type("application/json")
            .send(body.to_string().as_bytes())
            .map_err(|error| failure(format!("benchmark request failed: {error}")))?;
        drop(authorization);
        let status = response.status().as_u16();
        let mut text = String::new();
        response
            .body_mut()
            .as_reader()
            .take(MAX_BODY)
            .read_to_string(&mut text)
            .map_err(|error| failure(format!("cannot read the benchmark response: {error}")))?;
        if status != 200 {
            return Err(failure(format!(
                "benchmark request failed with HTTP {status}"
            )));
        }
        let value: Value = serde_json::from_str(&text)
            .map_err(|_| failure("the benchmark response is not JSON".into()))?;
        parse_timings(&value).ok_or_else(|| failure("the server reported no timings".into()))
    }
}

pub(crate) fn parse_timings(value: &Value) -> Option<CompletionTimings> {
    let timings = value.get("timings")?;
    let number = |key: &str| timings.get(key).and_then(Value::as_f64);
    let count = |key: &str| timings.get(key).and_then(Value::as_u64);
    Some(CompletionTimings {
        prompt_tokens: count("prompt_n")?,
        prompt_ms: number("prompt_ms")?,
        prompt_tokens_per_second: number("prompt_per_second").unwrap_or(0.0),
        generated_tokens: count("predicted_n")?,
        generation_ms: number("predicted_ms")?,
        generation_tokens_per_second: number("predicted_per_second").unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_server_timings_are_parsed() {
        let value = json!({"timings": {
            "prompt_n": 12, "prompt_ms": 30.5, "prompt_per_second": 393.4,
            "predicted_n": 32, "predicted_ms": 400.0, "predicted_per_second": 80.0
        }});
        let timings = parse_timings(&value).unwrap();
        assert_eq!((timings.prompt_tokens, timings.generated_tokens), (12, 32));
        assert!((timings.generation_tokens_per_second - 80.0).abs() < f64::EPSILON);
        assert!(parse_timings(&json!({"usage": {}})).is_none());
    }
}
