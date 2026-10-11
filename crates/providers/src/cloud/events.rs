use super::protocol;
use crate::*;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Assembler {
    text: String,
    calls: BTreeMap<u32, ToolCall>,
    usage: Usage,
    finish: Option<FinishReason>,
    pub done: bool,
}
impl Assembler {
    pub fn bedrock(
        &mut self,
        event: &str,
        payload: &str,
        emit: &mut dyn FnMut(&StreamEvent),
    ) -> Result<(), ProviderError> {
        if event.starts_with("exception:") {
            return Err(match event {
                "exception:throttlingException" => ProviderError::RateLimited { retry_after: None },
                "exception:validationException" => {
                    ProviderError::InvalidRequest("Bedrock rejected request".into())
                }
                _ => ProviderError::unavailable("Bedrock stream exception"),
            });
        }
        let v: Value = serde_json::from_str(payload).map_err(|_| protocol())?;
        match event {
            "contentBlockStart" if v["start"]["toolUse"].is_object() => self.call(
                index(&v, "contentBlockIndex")?,
                v["start"]["toolUse"]["toolUseId"].as_str(),
                v["start"]["toolUse"]["name"].as_str(),
                "",
                emit,
            )?,
            "contentBlockDelta" => {
                if let Some(text) = v["delta"]["text"].as_str() {
                    self.text(text, emit)?;
                }
                if let Some(input) = v["delta"]["toolUse"]["input"].as_str() {
                    self.call(index(&v, "contentBlockIndex")?, None, None, input, emit)?;
                }
            }
            "messageStop" => {
                self.finish = Some(reason(
                    ProviderKind::Bedrock,
                    v["stopReason"].as_str().ok_or_else(protocol)?,
                )?);
                self.done = true;
            }
            "metadata" => self.usage(
                number(&v["usage"]["inputTokens"]),
                number(&v["usage"]["outputTokens"]),
                emit,
            ),
            _ => {}
        }
        Ok(())
    }
    fn text(
        &mut self,
        text: &str,
        emit: &mut dyn FnMut(&StreamEvent),
    ) -> Result<(), ProviderError> {
        if self.text.len() + text.len() > 8 * 1024 * 1024 {
            return Err(protocol());
        }
        if !text.is_empty() {
            self.text.push_str(text);
            emit(&StreamEvent::TextDelta { text: text.into() });
        }
        Ok(())
    }
    fn call(
        &mut self,
        index: u32,
        id: Option<&str>,
        name: Option<&str>,
        args: &str,
        emit: &mut dyn FnMut(&StreamEvent),
    ) -> Result<(), ProviderError> {
        if index >= 256 || (self.calls.len() >= 64 && !self.calls.contains_key(&index)) {
            return Err(protocol());
        }
        let call = self.calls.entry(index).or_insert_with(|| ToolCall {
            provider_metadata: None,
            id: format!("call_{index}"),
            name: String::new(),
            arguments: String::new(),
        });
        if let Some(id) = id {
            if id.len() > 256 {
                return Err(protocol());
            }
            call.id = id.into();
        }
        if let Some(name) = name {
            if !is_tool_name(name) {
                return Err(protocol());
            }
            call.name = name.into();
        }
        if call.arguments.len() + args.len() > 1024 * 1024 {
            return Err(protocol());
        }
        call.arguments.push_str(args);
        emit(&StreamEvent::ToolCallDelta {
            index,
            id: id.map(str::to_owned),
            name: name.map(str::to_owned),
            arguments: args.into(),
        });
        Ok(())
    }
    fn usage(
        &mut self,
        input: Option<u32>,
        output: Option<u32>,
        emit: &mut dyn FnMut(&StreamEvent),
    ) {
        if input.is_some() {
            self.usage.prompt_tokens = input;
        }
        if output.is_some() {
            self.usage.completion_tokens = output;
        }
        if input.is_some() || output.is_some() {
            emit(&StreamEvent::Usage(self.usage));
        }
    }
    pub fn apply(
        &mut self,
        kind: ProviderKind,
        payload: &str,
        emit: &mut dyn FnMut(&StreamEvent),
    ) -> Result<(), ProviderError> {
        if kind == ProviderKind::AzureOpenai && payload.trim() == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let v: Value = serde_json::from_str(payload).map_err(|_| protocol())?;
        if v.get("error").is_some() || v["type"] == "error" {
            if kind == ProviderKind::OpenaiResponses {
                return Err(crate::openai::wire::stream_error(&v["error"]));
            }
            return Err(ProviderError::unavailable(
                "provider stream reported an error",
            ));
        }
        match kind {
            ProviderKind::OpenaiResponses => match v["type"].as_str().ok_or_else(protocol)? {
                "response.output_text.delta" => {
                    self.text(v["delta"].as_str().ok_or_else(protocol)?, emit)?
                }
                "response.output_item.added" if v["item"]["type"] == "function_call" => self.call(
                    index(&v, "output_index")?,
                    v["item"]["call_id"].as_str(),
                    v["item"]["name"].as_str(),
                    v["item"]["arguments"].as_str().unwrap_or(""),
                    emit,
                )?,
                "response.function_call_arguments.delta" => self.call(
                    index(&v, "output_index")?,
                    None,
                    None,
                    v["delta"].as_str().ok_or_else(protocol)?,
                    emit,
                )?,
                "response.completed" => {
                    self.done = true;
                    self.finish = Some(if self.calls.is_empty() {
                        FinishReason::Stop
                    } else {
                        FinishReason::ToolCalls
                    });
                    self.usage(
                        number(&v["response"]["usage"]["input_tokens"]),
                        number(&v["response"]["usage"]["output_tokens"]),
                        emit,
                    );
                }
                "response.incomplete" => {
                    self.done = true;
                    self.finish = Some(FinishReason::Length);
                }
                "response.failed" => {
                    return Err(crate::openai::wire::stream_error(&v["response"]["error"]));
                }
                _ => {}
            },
            ProviderKind::Anthropic => match v["type"].as_str().ok_or_else(protocol)? {
                "message_start" => self.usage(
                    number(&v["message"]["usage"]["input_tokens"]),
                    number(&v["message"]["usage"]["output_tokens"]),
                    emit,
                ),
                "content_block_start" if v["content_block"]["type"] == "tool_use" => self.call(
                    index(&v, "index")?,
                    v["content_block"]["id"].as_str(),
                    v["content_block"]["name"].as_str(),
                    "",
                    emit,
                )?,
                "content_block_delta" => match v["delta"]["type"].as_str().ok_or_else(protocol)? {
                    "text_delta" => {
                        self.text(v["delta"]["text"].as_str().ok_or_else(protocol)?, emit)?
                    }
                    "input_json_delta" => self.call(
                        index(&v, "index")?,
                        None,
                        None,
                        v["delta"]["partial_json"].as_str().ok_or_else(protocol)?,
                        emit,
                    )?,
                    _ => {}
                },
                "message_delta" => {
                    self.finish = Some(reason(
                        kind,
                        v["delta"]["stop_reason"].as_str().ok_or_else(protocol)?,
                    )?);
                    self.usage(
                        number(&v["usage"]["input_tokens"]),
                        number(&v["usage"]["output_tokens"]),
                        emit,
                    );
                }
                "message_stop" => self.done = true,
                _ => {}
            },
            ProviderKind::Gemini => {
                if v["promptFeedback"]["blockReason"].is_string() {
                    self.finish = Some(FinishReason::ContentFilter);
                    self.done = true;
                }
                if let Some(candidates) = v["candidates"].as_array()
                    && let Some(c) = candidates.first()
                {
                    if let Some(parts) = c["content"]["parts"].as_array() {
                        for p in parts {
                            if let Some(text) = p["text"].as_str()
                                && p["thought"] != true
                            {
                                self.text(text, emit)?;
                            }
                            if let Some(call) = p.get("functionCall") {
                                let index = self.calls.len() as u32;
                                self.call(
                                    index,
                                    call["id"].as_str(),
                                    call["name"].as_str(),
                                    &call["args"].to_string(),
                                    emit,
                                )?;
                                self.calls
                                    .get_mut(&index)
                                    .ok_or_else(protocol)?
                                    .provider_metadata =
                                    Some(serde_json::json!({"gemini":{"part":p}}));
                            }
                        }
                    }
                    if let Some(stop) = c["finishReason"].as_str() {
                        self.finish = Some(if stop == "STOP" && !self.calls.is_empty() {
                            FinishReason::ToolCalls
                        } else {
                            reason(kind, stop)?
                        });
                        self.done = true;
                    }
                }
                self.usage(
                    number(&v["usageMetadata"]["promptTokenCount"]),
                    number(&v["usageMetadata"]["candidatesTokenCount"]),
                    emit,
                );
            }
            ProviderKind::AzureOpenai => {
                if let Some(c) = v["choices"].as_array().and_then(|a| a.first()) {
                    if let Some(text) = c["delta"]["content"].as_str() {
                        self.text(text, emit)?;
                    }
                    if let Some(calls) = c["delta"]["tool_calls"].as_array() {
                        for call in calls {
                            self.call(
                                index(call, "index")?,
                                call["id"].as_str(),
                                call["function"]["name"].as_str(),
                                call["function"]["arguments"].as_str().unwrap_or(""),
                                emit,
                            )?;
                        }
                    }
                    if let Some(stop) = c["finish_reason"].as_str() {
                        self.finish = Some(reason(kind, stop)?);
                    }
                }
                self.usage(
                    number(&v["usage"]["prompt_tokens"]),
                    number(&v["usage"]["completion_tokens"]),
                    emit,
                );
            }
            _ => return Err(protocol()),
        }
        Ok(())
    }
    pub fn finish(self, emit: &mut dyn FnMut(&StreamEvent)) -> Result<ChatResponse, ProviderError> {
        if !self.done || self.finish.is_none() {
            return Err(protocol());
        }
        if self
            .calls
            .values()
            .any(|c| c.name.is_empty() || serde_json::from_str::<Value>(&c.arguments).is_err())
        {
            return Err(protocol());
        }
        let finish_reason = self.finish.ok_or_else(protocol)?;
        let mut message = ChatMessage::assistant(self.text);
        message.tool_calls = self.calls.into_values().collect();
        emit(&StreamEvent::Done { finish_reason });
        Ok(ChatResponse {
            message,
            finish_reason,
            usage: self.usage,
        })
    }
}
fn number(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}
fn index(v: &Value, field: &str) -> Result<u32, ProviderError> {
    number(&v[field]).ok_or_else(protocol)
}
fn reason(kind: ProviderKind, s: &str) -> Result<FinishReason, ProviderError> {
    let finish = match (kind, s) {
        (ProviderKind::Bedrock | ProviderKind::Anthropic, "end_turn" | "stop_sequence")
        | (ProviderKind::AzureOpenai, "stop")
        | (ProviderKind::Gemini, "STOP") => FinishReason::Stop,
        (ProviderKind::Bedrock | ProviderKind::Anthropic, "tool_use")
        | (ProviderKind::AzureOpenai, "tool_calls" | "function_call") => FinishReason::ToolCalls,
        (ProviderKind::Bedrock | ProviderKind::Anthropic, "max_tokens")
        | (ProviderKind::Gemini, "MAX_TOKENS")
        | (ProviderKind::AzureOpenai, "length") => FinishReason::Length,
        (ProviderKind::Bedrock, "guardrail_intervened" | "content_filtered")
        | (ProviderKind::Anthropic, "refusal")
        | (ProviderKind::AzureOpenai, "content_filter")
        | (
            ProviderKind::Gemini,
            "SAFETY"
            | "RECITATION"
            | "BLOCKLIST"
            | "PROHIBITED_CONTENT"
            | "SPII"
            | "IMAGE_SAFETY"
            | "IMAGE_PROHIBITED_CONTENT"
            | "IMAGE_RECITATION"
            | "ESCALATION"
            | "PUP_LIMITED_DISABLED",
        ) => FinishReason::ContentFilter,
        (ProviderKind::Bedrock | ProviderKind::Anthropic, "model_context_window_exceeded") => {
            return Err(ProviderError::ContextOverflow {
                limit: None,
                requested: None,
            });
        }
        (ProviderKind::Bedrock, "malformed_model_output" | "malformed_tool_use")
        | (
            ProviderKind::Gemini,
            "MALFORMED_FUNCTION_CALL"
            | "MALFORMED_RESPONSE"
            | "UNEXPECTED_TOOL_CALL"
            | "MISSING_THOUGHT_SIGNATURE"
            | "TOO_MANY_TOOL_CALLS",
        ) => {
            return Err(ProviderError::Protocol(
                "provider returned malformed generation".into(),
            ));
        }
        _ => {
            return Err(ProviderError::Protocol(
                "provider returned an invalid or unsupported finish reason".into(),
            ));
        }
    };
    Ok(finish)
}
