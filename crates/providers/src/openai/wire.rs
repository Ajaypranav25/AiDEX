//! OpenAI chat-completions wire format: request bodies, responses, and errors.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    ChatMessage, ChatRequest, ChatResponse, FinishReason, ModelId, ProviderError, ResponseFormat,
    Role, StreamEvent, ToolCall, ToolChoice, Usage,
};

/// Most models accepted from one `/models` listing.
const MAX_LISTED_MODELS: usize = 1000;

/// The JSON body for `POST /chat/completions`.
pub(crate) fn request_body(request: &ChatRequest, stream: bool) -> Vec<u8> {
    let mut body = Map::new();
    body.insert("model".into(), json!(request.model.as_str()));
    body.insert(
        "messages".into(),
        Value::Array(request.messages.iter().map(message).collect()),
    );
    body.insert("stream".into(), json!(stream));
    if stream {
        body.insert("stream_options".into(), json!({ "include_usage": true }));
    }
    if !request.tools.is_empty() {
        body.insert(
            "tools".into(),
            Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.parameters,
                            }
                        })
                    })
                    .collect(),
            ),
        );
        body.insert(
            "tool_choice".into(),
            match &request.tool_choice {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::None => json!("none"),
                ToolChoice::Required => json!("required"),
                ToolChoice::Tool { name } => {
                    json!({ "type": "function", "function": { "name": name } })
                }
            },
        );
    }
    if let Some(ResponseFormat::JsonSchema {
        name,
        schema,
        strict,
    }) = &request.response_format
    {
        body.insert(
            "response_format".into(),
            json!({
                "type": "json_schema",
                "json_schema": { "name": name, "schema": schema, "strict": strict },
            }),
        );
    }
    if let Some(temperature) = request.temperature {
        body.insert("temperature".into(), json!(temperature));
    }
    if let Some(max_tokens) = request.max_tokens {
        body.insert("max_tokens".into(), json!(max_tokens));
    }
    if !request.stop.is_empty() {
        body.insert("stop".into(), json!(request.stop));
    }
    // Serializing a `Value` cannot fail.
    serde_json::to_vec(&Value::Object(body)).unwrap_or_default()
}

fn message(message: &ChatMessage) -> Value {
    let mut wire = Map::new();
    wire.insert("role".into(), json!(message.role.as_str()));
    match message.role {
        Role::Assistant if !message.tool_calls.is_empty() => {
            wire.insert(
                "content".into(),
                if message.content.is_empty() {
                    Value::Null
                } else {
                    json!(message.content)
                },
            );
            wire.insert(
                "tool_calls".into(),
                Value::Array(
                    message
                        .tool_calls
                        .iter()
                        .map(|call| {
                            json!({
                                "id": call.id,
                                "type": "function",
                                "function": { "name": call.name, "arguments": call.arguments },
                            })
                        })
                        .collect(),
                ),
            );
        }
        Role::Tool => {
            wire.insert("tool_call_id".into(), json!(message.tool_call_id));
            wire.insert("content".into(), json!(message.content));
        }
        _ => {
            wire.insert("content".into(), json!(message.content));
        }
    }
    Value::Object(wire)
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct WireUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
}

impl From<WireUsage> for Usage {
    fn from(usage: WireUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
        }
    }
}

pub(crate) fn finish_reason(reason: &str) -> FinishReason {
    match reason {
        "length" => FinishReason::Length,
        "tool_calls" | "function_call" => FinishReason::ToolCalls,
        "content_filter" => FinishReason::ContentFilter,
        _ => FinishReason::Stop,
    }
}

#[derive(Deserialize)]
struct Completion {
    #[serde(default)]
    choices: Vec<CompletionChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
}

#[derive(Deserialize)]
struct CompletionChoice {
    #[serde(default)]
    index: u32,
    message: CompletionMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct CompletionMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<CompletionToolCall>,
}

#[derive(Deserialize)]
struct CompletionToolCall {
    #[serde(default)]
    id: Option<String>,
    function: CompletionFunction,
}

#[derive(Deserialize)]
struct CompletionFunction {
    name: String,
    #[serde(default)]
    arguments: Value,
}

/// Parse a non-streaming completion and replay it as stream events.
pub(crate) fn parse_completion(
    body: &str,
    on_event: &mut dyn FnMut(&StreamEvent),
) -> Result<ChatResponse, ProviderError> {
    let completion: Completion = serde_json::from_str(body)
        .map_err(|error| ProviderError::Protocol(format!("malformed completion ({error})")))?;
    let choice = completion
        .choices
        .into_iter()
        .find(|choice| choice.index == 0)
        .ok_or_else(|| ProviderError::Protocol("the completion has no choices".into()))?;
    let text = choice.message.content.unwrap_or_default();
    if !text.is_empty() {
        on_event(&StreamEvent::TextDelta { text: text.clone() });
    }
    let mut tool_calls = Vec::new();
    for (index, call) in choice.message.tool_calls.into_iter().enumerate() {
        if call.function.name.is_empty() {
            return Err(ProviderError::Protocol(
                "a tool call has no function name".into(),
            ));
        }
        let arguments = match call.function.arguments {
            Value::String(text) => text,
            Value::Null => String::new(),
            other => other.to_string(),
        };
        let call = ToolCall {
            id: call
                .id
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| format!("call_{index}")),
            name: call.function.name,
            arguments,
        };
        on_event(&StreamEvent::ToolCallDelta {
            index: u32::try_from(index).unwrap_or(u32::MAX),
            id: Some(call.id.clone()),
            name: Some(call.name.clone()),
            arguments: call.arguments.clone(),
        });
        tool_calls.push(call);
    }
    let usage: Usage = completion.usage.unwrap_or_default().into();
    on_event(&StreamEvent::Usage(usage));
    let finish_reason = match choice.finish_reason.as_deref().map(finish_reason) {
        Some(FinishReason::Stop) | None if !tool_calls.is_empty() => FinishReason::ToolCalls,
        Some(reason) => reason,
        None => FinishReason::Stop,
    };
    on_event(&StreamEvent::Done { finish_reason });
    let mut message = ChatMessage::assistant(text);
    message.tool_calls = tool_calls;
    Ok(ChatResponse {
        message,
        finish_reason,
        usage,
    })
}

/// A model reported by `GET /models`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredModel {
    pub id: ModelId,
    /// Context length when the server reports one (`context_length`,
    /// `max_model_len`, or llama.cpp `meta.n_ctx`/`meta.n_ctx_train`).
    pub context_window: Option<u32>,
}

#[derive(Deserialize)]
struct ModelList {
    data: Vec<Value>,
}

/// Parse a `/models` listing; entries with unusable ids are skipped.
pub(crate) fn parse_models(body: &str) -> Result<Vec<DiscoveredModel>, ProviderError> {
    let list: ModelList = serde_json::from_str(body)
        .map_err(|error| ProviderError::Protocol(format!("malformed model list ({error})")))?;
    if list.data.len() > MAX_LISTED_MODELS {
        return Err(ProviderError::Protocol(
            "the model list is larger than 1000 entries".into(),
        ));
    }
    Ok(list
        .data
        .iter()
        .filter_map(|entry| {
            let id = ModelId::new(entry.get("id")?.as_str()?).ok()?;
            let context = |value: Option<&Value>| {
                value
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n > 0)
            };
            let meta = entry.get("meta");
            let context_window = context(entry.get("context_length"))
                .or_else(|| context(entry.get("max_model_len")))
                .or_else(|| context(entry.get("context_window")))
                .or_else(|| context(meta.and_then(|meta| meta.get("n_ctx"))))
                .or_else(|| context(meta.and_then(|meta| meta.get("n_ctx_train"))));
            Some(DiscoveredModel { id, context_window })
        })
        .collect())
}

/// Map a failed HTTP status (and its error body, parsed for machine codes
/// only) to a provider error. Server-supplied text is never copied into it.
pub(crate) fn http_error(
    status: u16,
    retry_after: Option<std::time::Duration>,
    body: &str,
    model: Option<&ModelId>,
) -> ProviderError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let error = parsed.as_ref().and_then(|value| {
        value
            .get("error")
            .filter(|error| error.is_object())
            .or(Some(value))
    });
    classify(status, error, retry_after, model)
}

/// An `{"error": ...}` object received inside a stream.
pub(crate) fn stream_error(error: &Value) -> ProviderError {
    let status = error
        .get("code")
        .and_then(Value::as_u64)
        .and_then(|code| u16::try_from(code).ok())
        .filter(|code| (400..600).contains(code))
        .unwrap_or(500);
    classify(status, Some(error), None, None)
}

fn classify(
    status: u16,
    error: Option<&Value>,
    retry_after: Option<std::time::Duration>,
    model: Option<&ModelId>,
) -> ProviderError {
    let field = |name: &str| {
        error
            .and_then(|error| error.get(name))
            .and_then(Value::as_str)
    };
    let code = field("code")
        .or_else(|| field("type"))
        .and_then(sanitize_code);
    let kind = field("type").unwrap_or_default();
    let message = field("message").unwrap_or_default().to_ascii_lowercase();
    let number = |name: &str| {
        error
            .and_then(|error| error.get(name))
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    let context_overflow = code.as_deref() == Some("context_length_exceeded")
        || kind == "exceed_context_size_error"
        || [
            "context length",
            "context size",
            "context window",
            "maximum context",
        ]
        .iter()
        .any(|needle| message.contains(needle));
    match status {
        401 | 403 => ProviderError::Unauthorized,
        408 => ProviderError::Timeout,
        429 => ProviderError::RateLimited { retry_after },
        400 | 413 if context_overflow => ProviderError::ContextOverflow {
            limit: number("n_ctx"),
            requested: number("n_prompt_tokens"),
        },
        404 if code.as_deref().is_some_and(|code| code.contains("model"))
            || message.contains("model") =>
        {
            ProviderError::ModelNotFound {
                model: model.map(ToString::to_string).unwrap_or_default(),
            }
        }
        503 => ProviderError::unavailable("the provider is overloaded or still loading (HTTP 503)"),
        _ => ProviderError::Http { status, code },
    }
}

/// Keep short identifier-like codes; drop anything that could be free text.
fn sanitize_code(code: &str) -> Option<String> {
    (!code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')))
    .then(|| code.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelId, ToolSpec};

    #[test]
    fn request_bodies_follow_the_openai_schema() {
        let mut request = ChatRequest::new(
            ModelId::new("gpt-x").unwrap(),
            vec![
                ChatMessage::system("be brief"),
                ChatMessage::user("read a"),
                ChatMessage {
                    tool_calls: vec![ToolCall {
                        id: "c1".into(),
                        name: "read_file".into(),
                        arguments: "{\"path\":\"a\"}".into(),
                    }],
                    ..ChatMessage::assistant("")
                },
                ChatMessage::tool_result("c1", "contents"),
            ],
        )
        .with_tools(vec![ToolSpec {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: json!({"type": "object"}),
        }])
        .with_response_format(ResponseFormat::JsonSchema {
            name: "action".into(),
            schema: json!({"type": "object"}),
            strict: true,
        })
        .with_max_tokens(64);
        request.stop = vec!["END".into()];
        let body: Value = serde_json::from_slice(&request_body(&request, true)).unwrap();
        assert_eq!(body["model"], "gpt-x");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["messages"][2]["content"], Value::Null);
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
        assert_eq!(body["messages"][3]["tool_call_id"], "c1");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["name"], "action");
        assert_eq!(body["max_tokens"], 64);
        assert_eq!(body["stop"][0], "END");
        let plain: Value = serde_json::from_slice(&request_body(
            &ChatRequest::new(ModelId::new("m").unwrap(), vec![ChatMessage::user("x")]),
            false,
        ))
        .unwrap();
        assert!(plain.get("tools").is_none() && plain.get("stream_options").is_none());
    }

    #[test]
    fn model_listings_report_known_context_fields() {
        let models = parse_models(
            r#"{"object":"list","data":[
                {"id":"gpt-4o","object":"model"},
                {"id":"meta-llama/Llama-3.1-8B","max_model_len":131072},
                {"id":"qwen","meta":{"n_ctx_train":32768}},
                {"id":"bad id with spaces"},
                {"id":"router/x","context_length":8192}
            ]}"#,
        )
        .unwrap();
        let summary: Vec<_> = models
            .iter()
            .map(|model| (model.id.as_str(), model.context_window))
            .collect();
        assert_eq!(
            summary,
            [
                ("gpt-4o", None),
                ("meta-llama/Llama-3.1-8B", Some(131_072)),
                ("qwen", Some(32_768)),
                ("router/x", Some(8192))
            ]
        );
        assert!(parse_models("{}").is_err());
    }

    #[test]
    fn errors_map_to_first_class_outcomes_without_server_text() {
        let model = ModelId::new("m").unwrap();
        let echoed = r#"{"error":{"message":"Incorrect API key provided: sk-live-123","type":"invalid_request_error","code":"invalid_api_key"}}"#;
        let unauthorized = http_error(401, None, echoed, Some(&model));
        assert_eq!(unauthorized, ProviderError::Unauthorized);
        assert!(!format!("{unauthorized} {unauthorized:?}").contains("sk-live-123"));
        assert_eq!(
            http_error(429, Some(std::time::Duration::from_secs(7)), "", None),
            ProviderError::RateLimited {
                retry_after: Some(std::time::Duration::from_secs(7))
            }
        );
        assert_eq!(
            http_error(
                400,
                None,
                r#"{"error":{"code":400,"message":"the request exceeds the available context size","type":"exceed_context_size_error","n_prompt_tokens":5000,"n_ctx":4096}}"#,
                None
            ),
            ProviderError::ContextOverflow {
                limit: Some(4096),
                requested: Some(5000)
            }
        );
        assert!(matches!(
            http_error(
                404,
                None,
                r#"{"error":{"code":"model_not_found"}}"#,
                Some(&model)
            ),
            ProviderError::ModelNotFound { .. }
        ));
        let free_text = http_error(
            500,
            None,
            r#"{"error":{"code":"has spaces and sk-1"}}"#,
            None,
        );
        assert_eq!(
            free_text,
            ProviderError::Http {
                status: 500,
                code: None
            }
        );
        assert!(matches!(
            http_error(503, None, "", None),
            ProviderError::Unavailable { .. }
        ));
    }

    #[test]
    fn non_streaming_completions_replay_as_events() {
        let mut events = Vec::new();
        let response = parse_completion(
            r#"{"choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"x","type":"function","function":{"name":"f","arguments":{"a":1}}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":3,"completion_tokens":2}}"#,
            &mut |event| events.push(event.clone()),
        )
        .unwrap();
        assert_eq!(response.message.tool_calls[0].arguments, r#"{"a":1}"#);
        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
        assert_eq!(events.len(), 3);
    }
}
