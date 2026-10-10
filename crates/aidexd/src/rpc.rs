//! JSON-RPC 2.0 message model for the AiDEX IPC contract (version 2).
//!
//! Wire format: UTF-8, one JSON-RPC 2.0 object per `\n`-terminated line, at
//! most [`MAX_MESSAGE_BYTES`] per line. Batches are not supported. Members
//! outside the JSON-RPC 2.0 envelope are rejected.

use crate::errors::{ErrorCode, RpcError};
use serde::{Serialize, Serializer, de::DeserializeOwned};
use serde_json::{Map, Value};

/// IPC contract version spoken by this build (`aidex.handshake` and endpoint file).
pub const CONTRACT_VERSION: u32 = 2;
/// IPC contract versions this build accepts.
pub const SUPPORTED_CONTRACT_VERSIONS: &[u32] = &[CONTRACT_VERSION];
/// Maximum size of one message line in bytes, excluding the newline.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
/// Maximum method-name length.
pub const MAX_METHOD_BYTES: usize = 128;

/// Transport-owned method names and notifications use this prefix.
pub const RESERVED_PREFIX: &str = "aidex.";
/// First request on every connection.
pub const HANDSHAKE_METHOD: &str = "aidex.handshake";
/// Ends a stream subscription: params `{ "subscription": "<id>" }`.
pub const UNSUBSCRIBE_METHOD: &str = "aidex.unsubscribe";
/// Sent before the daemon closes a connection: params `{ "code", "message" }`.
pub const CLOSING_NOTIFICATION: &str = "aidex.closing";
/// Sent when the daemon ends a subscription: params `{ "subscription", "reason" }`.
pub const UNSUBSCRIBED_NOTIFICATION: &str = "aidex.unsubscribed";

const JSONRPC: &str = "2.0";

/// A request id. AiDEX accepts strings, integers, and (discouraged) `null`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Id {
    Number(i64),
    String(String),
    Null,
}

impl Id {
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Null => Some(Self::Null),
            Value::String(text) => Some(Self::String(text.clone())),
            Value::Number(number) => number.as_i64().map(Self::Number),
            _ => None,
        }
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Number(number) => serializer.serialize_i64(*number),
            Self::String(text) => serializer.serialize_str(text),
            Self::Null => serializer.serialize_unit(),
        }
    }
}

/// One decoded JSON-RPC 2.0 message.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Request {
        id: Id,
        method: String,
        params: Option<Value>,
    },
    Notification {
        method: String,
        params: Option<Value>,
    },
    Response {
        id: Id,
        outcome: Result<Value, RpcError>,
    },
}

/// A message that could not be accepted, with the best-effort id to answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Malformed {
    pub id: Id,
    pub error: RpcError,
}

#[derive(Serialize)]
struct Wire<'a> {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a RpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a Id>,
}

impl Message {
    pub fn request(id: i64, method: &str, params: Value) -> Self {
        Self::Request {
            id: Id::Number(id),
            method: method.to_owned(),
            params: structured(params),
        }
    }

    pub fn notification(method: &str, params: Value) -> Self {
        Self::Notification {
            method: method.to_owned(),
            params: structured(params),
        }
    }

    pub fn response(id: Id, outcome: Result<Value, RpcError>) -> Self {
        Self::Response { id, outcome }
    }

    /// Decode one line (without its newline).
    pub fn parse(bytes: &[u8]) -> Result<Self, Malformed> {
        let value: Value = serde_json::from_slice(bytes).map_err(|error| Malformed {
            id: Id::Null,
            error: RpcError::parse_error(format!("message is not valid UTF-8 JSON: {error}")),
        })?;
        Self::from_value(value)
    }

    fn from_value(value: Value) -> Result<Self, Malformed> {
        let mut object = match value {
            Value::Object(object) => object,
            Value::Array(_) => return Err(invalid(Id::Null, "batch messages are not supported")),
            _ => return Err(invalid(Id::Null, "message must be a JSON object")),
        };
        let id = match object.remove("id") {
            None => None,
            Some(value) => Some(
                Id::from_value(&value)
                    .ok_or_else(|| invalid(Id::Null, "id must be a string, an integer, or null"))?,
            ),
        };
        let reply_id = id.clone().unwrap_or(Id::Null);
        match object.remove("jsonrpc") {
            Some(Value::String(version)) if version == JSONRPC => {}
            _ => return Err(invalid(reply_id, "jsonrpc member must be \"2.0\"")),
        }
        let method = object.remove("method");
        let params = object.remove("params");
        let result = object.remove("result");
        let error = object.remove("error");
        if !object.is_empty() {
            return Err(invalid(
                reply_id,
                "message contains members outside the JSON-RPC 2.0 envelope",
            ));
        }
        match (method, result, error) {
            (Some(method), None, None) => {
                let Value::String(method) = method else {
                    return Err(invalid(reply_id, "method must be a string"));
                };
                if method.is_empty() || method.len() > MAX_METHOD_BYTES {
                    return Err(invalid(
                        reply_id,
                        format!("method must be 1 to {MAX_METHOD_BYTES} bytes"),
                    ));
                }
                if params
                    .as_ref()
                    .is_some_and(|params| !params.is_object() && !params.is_array())
                {
                    return Err(invalid(reply_id, "params must be an object or an array"));
                }
                Ok(match id {
                    Some(id) => Self::Request { id, method, params },
                    None => Self::Notification { method, params },
                })
            }
            (None, Some(result), None) if params.is_none() => {
                let id = id.ok_or_else(|| invalid(Id::Null, "response is missing its id"))?;
                Ok(Self::Response {
                    id,
                    outcome: Ok(result),
                })
            }
            (None, None, Some(error)) if params.is_none() => {
                let id = id.ok_or_else(|| invalid(Id::Null, "response is missing its id"))?;
                let error: RpcError = serde_json::from_value(error)
                    .map_err(|_| invalid(id.clone(), "response error object is malformed"))?;
                Ok(Self::Response {
                    id,
                    outcome: Err(error),
                })
            }
            _ => Err(invalid(
                reply_id,
                "message must be exactly one request, notification, or response",
            )),
        }
    }

    /// Encode as one newline-terminated line.
    pub fn encode_line(&self) -> String {
        let wire = match self {
            Self::Request { id, method, params } => Wire {
                jsonrpc: JSONRPC,
                method: Some(method),
                params: params.as_ref(),
                result: None,
                error: None,
                id: Some(id),
            },
            Self::Notification { method, params } => Wire {
                jsonrpc: JSONRPC,
                method: Some(method),
                params: params.as_ref(),
                result: None,
                error: None,
                id: None,
            },
            Self::Response { id, outcome } => Wire {
                jsonrpc: JSONRPC,
                method: None,
                params: None,
                result: outcome.as_ref().ok(),
                error: outcome.as_ref().err(),
                id: Some(id),
            },
        };
        // `Value` maps always have string keys, so serialization cannot fail.
        let mut line = serde_json::to_string(&wire).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"message encoding failed"},"id":null}"#
                .to_owned()
        });
        line.push('\n');
        line
    }
}

/// `null` means "no params"; scalars are kept so the receiver rejects them.
fn structured(params: Value) -> Option<Value> {
    match params {
        Value::Null => None,
        other => Some(other),
    }
}

fn invalid(id: Id, message: impl Into<String>) -> Malformed {
    Malformed {
        id,
        error: RpcError::invalid_request(message),
    }
}

/// Decode handler params into a typed struct; absent params decode as `{}`.
///
/// Use `#[serde(deny_unknown_fields)]` on `T` so unknown members are rejected.
pub fn parse_params<T: DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    let params = match params {
        Value::Null => Value::Object(Map::new()),
        other => other,
    };
    serde_json::from_value(params).map_err(|error| RpcError::invalid_params(error.to_string()))
}

/// Reject any params other than absent, `{}`, or `[]`.
pub fn expect_no_params(params: &Value) -> Result<(), RpcError> {
    let empty = match params {
        Value::Null => true,
        Value::Object(object) => object.is_empty(),
        Value::Array(items) => items.is_empty(),
        _ => false,
    };
    if empty {
        Ok(())
    } else {
        Err(RpcError::invalid_params("this method takes no parameters"))
    }
}

/// Serialize a handler result.
pub fn to_result<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value)
        .map_err(|error| RpcError::internal(format!("cannot encode result: {error}")))
}

/// Whether `method` follows the `<domain>.<verb>` naming convention.
pub fn is_valid_method_name(method: &str) -> bool {
    method.len() <= MAX_METHOD_BYTES
        && method.split('.').count() >= 2
        && method.split('.').all(|segment| {
            let mut chars = segment.chars();
            chars.next().is_some_and(|c| c.is_ascii_lowercase())
                && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

pub(crate) fn error_line(id: Id, kind: ErrorCode, message: impl Into<String>) -> String {
    Message::response(id, Err(RpcError::new(kind, message))).encode_line()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(text: &str) -> Result<Message, Malformed> {
        Message::parse(text.as_bytes())
    }

    #[test]
    fn requests_notifications_and_responses_round_trip() {
        for message in [
            Message::request(7, "daemon.status", json!({})),
            Message::notification("events.event", json!({"subscription": "s1", "data": 1})),
            Message::response(Id::String("a".into()), Ok(json!({"ok": true}))),
            Message::response(Id::Null, Err(RpcError::new(ErrorCode::NotFound, "missing"))),
        ] {
            let line = message.encode_line();
            assert!(line.ends_with('\n') && !line[..line.len() - 1].contains('\n'));
            assert_eq!(parse(line.trim_end()).unwrap(), message);
        }
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        let error = parse("{not json").unwrap_err();
        assert_eq!(error.id, Id::Null);
        assert_eq!(error.error.kind(), Some(ErrorCode::ParseError));
        let invalid_utf8 = Message::parse(&[b'"', 0xff, b'"']).unwrap_err();
        assert_eq!(invalid_utf8.error.code, -32700);
    }

    #[test]
    fn envelope_violations_are_invalid_requests_with_best_effort_id() {
        for (text, id) in [
            (r#"[{"jsonrpc":"2.0","method":"a.b","id":1}]"#, Id::Null),
            (r#""text""#, Id::Null),
            (r#"{"jsonrpc":"1.0","method":"a.b","id":1}"#, Id::Number(1)),
            (
                r#"{"jsonrpc":"2.0","method":"a.b","id":2,"x":1}"#,
                Id::Number(2),
            ),
            (r#"{"jsonrpc":"2.0","method":5,"id":3}"#, Id::Number(3)),
            (
                r#"{"jsonrpc":"2.0","method":"a.b","params":3,"id":4}"#,
                Id::Number(4),
            ),
            (r#"{"jsonrpc":"2.0","method":"a.b","id":1.5}"#, Id::Null),
            (r#"{"jsonrpc":"2.0","id":5}"#, Id::Number(5)),
            (r#"{"jsonrpc":"2.0","result":1}"#, Id::Null),
        ] {
            let error = parse(text).unwrap_err();
            assert_eq!(
                error.error.kind(),
                Some(ErrorCode::InvalidRequest),
                "{text}"
            );
            assert_eq!(error.id, id, "{text}");
        }
    }

    #[test]
    fn params_helpers_reject_unexpected_input() {
        #[derive(serde::Deserialize, Debug)]
        #[serde(deny_unknown_fields)]
        struct Params {
            #[serde(default)]
            limit: Option<u32>,
        }
        assert_eq!(parse_params::<Params>(Value::Null).unwrap().limit, None);
        assert_eq!(
            parse_params::<Params>(json!({"limit": 3})).unwrap().limit,
            Some(3)
        );
        assert_eq!(
            parse_params::<Params>(json!({"other": 1}))
                .unwrap_err()
                .kind(),
            Some(ErrorCode::InvalidParams)
        );
        assert!(expect_no_params(&Value::Null).is_ok());
        assert!(expect_no_params(&json!({})).is_ok());
        assert!(expect_no_params(&json!({"a": 1})).is_err());
    }

    #[test]
    fn method_names_follow_domain_verb_convention() {
        for valid in [
            "daemon.status",
            "events.subscribe",
            "models.list_local",
            "a.b.c2",
        ] {
            assert!(is_valid_method_name(valid), "{valid}");
        }
        for invalid in [
            "status",
            "Daemon.status",
            "daemon.",
            ".x",
            "daemon.st-atus",
            "1a.b",
        ] {
            assert!(!is_valid_method_name(invalid), "{invalid}");
        }
    }
}
