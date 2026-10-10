//! `aidex.handshake`: the mandatory first request on every connection.
//!
//! Params: `{ "contract_version": 2, "token": "<64 hex>", "client": { "name", "version" } }`.
//! The version is checked before the capability so a newer or older client
//! learns which versions are supported; the capability comparison is
//! constant-time. Any failure is answered once and the connection is closed.

use crate::{
    endpoint::Capability,
    errors::{ErrorCode, RpcError},
    router::ClientInfo,
    rpc::{HANDSHAKE_METHOD, Id, Malformed, Message, SUPPORTED_CONTRACT_VERSIONS},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;

/// Largest accepted handshake line.
pub(crate) const MAX_HANDSHAKE_BYTES: usize = 4096;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeParams {
    pub contract_version: u32,
    pub token: String,
    pub client: ClientInfo,
}

impl fmt::Debug for HandshakeParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandshakeParams")
            .field("contract_version", &self.contract_version)
            .field("token", &"<redacted>")
            .field("client", &self.client)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

impl ServerInfo {
    pub fn current() -> Self {
        Self {
            name: "aidexd".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// Successful handshake result.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HandshakeResult {
    pub contract_version: u32,
    pub server: ServerInfo,
    pub connection_id: u64,
}

fn reject(id: &Id, kind: ErrorCode, message: impl Into<String>) -> Malformed {
    Malformed {
        id: id.clone(),
        error: RpcError::new(kind, message),
    }
}

/// Validate the first line of a connection. Returns the request id and client.
pub(crate) fn authenticate(line: &[u8], token: &Capability) -> Result<(Id, ClientInfo), Malformed> {
    let (id, params) = match Message::parse(line)? {
        Message::Request { id, method, params } if method == HANDSHAKE_METHOD => {
            (id, params.unwrap_or(Value::Null))
        }
        Message::Request { id, .. } => {
            return Err(reject(
                &id,
                ErrorCode::HandshakeRequired,
                "the first request must be `aidex.handshake`",
            ));
        }
        _ => {
            return Err(reject(
                &Id::Null,
                ErrorCode::HandshakeRequired,
                "the first message must be an `aidex.handshake` request",
            ));
        }
    };
    let requested = params.get("contract_version").and_then(Value::as_u64);
    match requested {
        None => {
            return Err(reject(
                &id,
                ErrorCode::InvalidParams,
                "handshake requires an integer contract_version",
            ));
        }
        Some(version)
            if !SUPPORTED_CONTRACT_VERSIONS
                .iter()
                .any(|supported| u64::from(*supported) == version) =>
        {
            return Err(Malformed {
                id,
                error: RpcError::new(
                    ErrorCode::IncompatibleVersion,
                    format!(
                        "IPC contract version {version} is not supported; supported versions: {SUPPORTED_CONTRACT_VERSIONS:?}"
                    ),
                )
                .with_data(json!({ "supported": SUPPORTED_CONTRACT_VERSIONS, "requested": version })),
            });
        }
        Some(_) => {}
    }
    let params: HandshakeParams = serde_json::from_value(params).map_err(|_| {
        reject(
            &id,
            ErrorCode::InvalidParams,
            "handshake params must be exactly {contract_version, token, client: {name, version}}",
        )
    })?;
    if !token.matches(&params.token) {
        return Err(reject(
            &id,
            ErrorCode::Unauthorized,
            "daemon capability was not accepted",
        ));
    }
    params
        .client
        .validate()
        .map_err(|message| reject(&id, ErrorCode::InvalidParams, message))?;
    Ok((id, params.client))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> Capability {
        Capability::parse("c".repeat(64)).unwrap()
    }

    fn line(params: Value) -> Vec<u8> {
        Message::request(1, HANDSHAKE_METHOD, params)
            .encode_line()
            .into_bytes()
    }

    fn valid() -> Value {
        json!({"contract_version": 2, "token": "c".repeat(64), "client": {"name": "t", "version": "1"}})
    }

    #[test]
    fn accepts_the_current_version_with_the_right_capability() {
        let (id, client) = authenticate(line(valid()).trim_ascii_end(), &token()).unwrap();
        assert_eq!(id, Id::Number(1));
        assert_eq!(client, ClientInfo::new("t", "1"));
    }

    #[test]
    fn rejects_wrong_token_version_shape_and_first_method() {
        let mut wrong_token = valid();
        wrong_token["token"] = json!("d".repeat(64));
        let mut old = valid();
        old["contract_version"] = json!(1);
        let mut extra = valid();
        extra["extra"] = json!(true);
        let cases = [
            (line(wrong_token), ErrorCode::Unauthorized),
            (line(old), ErrorCode::IncompatibleVersion),
            (line(extra), ErrorCode::InvalidParams),
            (line(json!({"token": "x"})), ErrorCode::InvalidParams),
            (
                Message::request(1, "daemon.status", Value::Null)
                    .encode_line()
                    .into_bytes(),
                ErrorCode::HandshakeRequired,
            ),
            (
                Message::notification(HANDSHAKE_METHOD, valid())
                    .encode_line()
                    .into_bytes(),
                ErrorCode::HandshakeRequired,
            ),
            (b"{oops".to_vec(), ErrorCode::ParseError),
        ];
        for (bytes, expected) in cases {
            let error = authenticate(bytes.trim_ascii_end(), &token()).unwrap_err();
            assert_eq!(error.error.kind(), Some(expected));
            assert!(!error.error.message.contains(&"c".repeat(64)));
        }
        let mut old = valid();
        old["contract_version"] = json!(1);
        let error = authenticate(line(old).trim_ascii_end(), &token()).unwrap_err();
        assert_eq!(error.error.data.unwrap()["supported"], json!([2]));
    }

    #[test]
    fn debug_output_redacts_the_capability() {
        let params: HandshakeParams = serde_json::from_value(valid()).unwrap();
        assert!(!format!("{params:?}").contains(&"c".repeat(64)));
    }
}
