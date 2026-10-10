//! Server-sent events decoding and OpenAI chat-chunk accumulation.

use serde::Deserialize;

use crate::{ChatMessage, ChatResponse, FinishReason, ProviderError, StreamEvent, ToolCall, Usage};

use super::wire::{WireUsage, finish_reason, stream_error};

/// Largest assembled text accepted from one response.
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
/// Most tool calls accepted from one response.
const MAX_TOOL_CALLS: usize = 64;
/// Largest arguments string accepted for one tool call.
const MAX_ARGUMENT_BYTES: usize = 1024 * 1024;
/// Data accumulated across individually legal SSE lines before dispatch.
const MAX_EVENT_BYTES: usize = 1024 * 1024;

/// Incremental SSE parser (WHATWG event-stream rules for the `data` field).
#[derive(Debug, Default)]
pub(crate) struct SseDecoder {
    data: String,
    has_data: bool,
}

impl SseDecoder {
    /// Feed one line; returns the event payload completed by a blank line.
    pub fn push_line(&mut self, line: &str) -> Result<Option<String>, ProviderError> {
        if line.is_empty() {
            return Ok(self.flush());
        }
        if line.starts_with(':') {
            return Ok(None);
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        if field == "data" {
            let value = value.strip_prefix(' ').unwrap_or(value);
            if self
                .data
                .len()
                .checked_add(value.len())
                .and_then(|len| len.checked_add(usize::from(self.has_data)))
                .is_none_or(|len| len > MAX_EVENT_BYTES)
            {
                self.data.clear();
                self.has_data = false;
                return Err(ProviderError::Protocol(
                    "SSE event exceeds the size limit".into(),
                ));
            }
            if self.has_data {
                self.data.push('\n');
            }
            self.data.push_str(value);
            self.has_data = true;
        }
        Ok(None)
    }

    /// The pending payload at end of stream, if the server omitted the final blank line.
    pub fn flush(&mut self) -> Option<String> {
        if !self.has_data {
            return None;
        }
        self.has_data = false;
        Some(std::mem::take(&mut self.data))
    }
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<WireUsage>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    delta: Option<Delta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallChunk>>,
}

#[derive(Deserialize)]
struct ToolCallChunk {
    #[serde(default)]
    index: Option<u32>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<FunctionChunk>,
}

#[derive(Deserialize)]
struct FunctionChunk {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct PartialCall {
    id: Option<String>,
    name: String,
    arguments: String,
}

/// Builds a [`ChatResponse`] from streamed chunks, forwarding deltas.
#[derive(Debug, Default)]
pub(crate) struct StreamAccumulator {
    text: String,
    calls: Vec<PartialCall>,
    finish: Option<FinishReason>,
    usage: Usage,
    done: bool,
}

impl StreamAccumulator {
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Apply one SSE payload (`[DONE]` or a JSON chunk).
    pub fn apply(
        &mut self,
        payload: &str,
        on_event: &mut dyn FnMut(&StreamEvent),
    ) -> Result<(), ProviderError> {
        if payload.trim() == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let chunk: Chunk = serde_json::from_str(payload)
            .map_err(|_| ProviderError::Protocol("malformed stream chunk".into()))?;
        if let Some(error) = chunk.error {
            return Err(stream_error(&error));
        }
        for choice in chunk.choices.into_iter().filter(|choice| choice.index == 0) {
            if let Some(delta) = choice.delta {
                self.apply_delta(delta, on_event)?;
            }
            if let Some(reason) = choice.finish_reason {
                self.finish = Some(finish_reason(&reason));
            }
        }
        if let Some(usage) = chunk.usage {
            self.usage = usage.into();
            on_event(&StreamEvent::Usage(self.usage));
        }
        Ok(())
    }

    fn apply_delta(
        &mut self,
        delta: Delta,
        on_event: &mut dyn FnMut(&StreamEvent),
    ) -> Result<(), ProviderError> {
        if let Some(text) = delta.content.filter(|text| !text.is_empty()) {
            if self.text.len() + text.len() > MAX_TEXT_BYTES {
                return Err(ProviderError::Protocol(
                    "response text exceeds the size limit".into(),
                ));
            }
            self.text.push_str(&text);
            on_event(&StreamEvent::TextDelta { text });
        }
        for chunk in delta.tool_calls.unwrap_or_default() {
            let index = self.call_index(&chunk)?;
            let call = &mut self.calls[index];
            let id = chunk.id.filter(|id| !id.is_empty() && call.id.is_none());
            if let Some(id) = &id {
                call.id = Some(id.clone());
            }
            let (name, arguments) = chunk
                .function
                .map(|function| (function.name, function.arguments.unwrap_or_default()))
                .unwrap_or_default();
            let name = name.filter(|name| !name.is_empty());
            if let Some(name) = &name {
                call.name.push_str(name);
            }
            if call.arguments.len() + arguments.len() > MAX_ARGUMENT_BYTES {
                return Err(ProviderError::Protocol(
                    "tool call arguments exceed the size limit".into(),
                ));
            }
            call.arguments.push_str(&arguments);
            on_event(&StreamEvent::ToolCallDelta {
                index: u32::try_from(index).unwrap_or(u32::MAX),
                id,
                name,
                arguments,
            });
        }
        Ok(())
    }

    /// Resolve which call a chunk extends; servers that omit `index` extend the
    /// latest call unless the chunk starts a new id.
    fn call_index(&mut self, chunk: &ToolCallChunk) -> Result<usize, ProviderError> {
        let index = match chunk.index {
            Some(index) => usize::try_from(index).unwrap_or(usize::MAX),
            None => {
                let starts_new = chunk.id.as_deref().is_some_and(|id| {
                    !id.is_empty()
                        && self
                            .calls
                            .last()
                            .is_none_or(|last| last.id.as_deref() != Some(id))
                });
                if starts_new || self.calls.is_empty() {
                    self.calls.len()
                } else {
                    self.calls.len() - 1
                }
            }
        };
        if index >= MAX_TOOL_CALLS {
            return Err(ProviderError::Protocol(
                "the response contains too many tool calls".into(),
            ));
        }
        while self.calls.len() <= index {
            self.calls.push(PartialCall::default());
        }
        Ok(index)
    }

    /// Assemble the response. `completed` is false when the stream ended
    /// without `[DONE]` or a finish reason.
    pub fn finish(
        self,
        on_event: &mut dyn FnMut(&StreamEvent),
    ) -> Result<ChatResponse, ProviderError> {
        if !self.done && self.finish.is_none() {
            return Err(ProviderError::Protocol(
                "the stream ended before the response completed".into(),
            ));
        }
        let mut tool_calls = Vec::with_capacity(self.calls.len());
        for (index, call) in self.calls.into_iter().enumerate() {
            if call.name.is_empty() {
                return Err(ProviderError::Protocol(
                    "a streamed tool call has no function name".into(),
                ));
            }
            tool_calls.push(ToolCall {
                provider_metadata: None,
                id: call.id.unwrap_or_else(|| format!("call_{index}")),
                name: call.name,
                arguments: call.arguments,
            });
        }
        let finish_reason = match self.finish {
            Some(FinishReason::Stop) | None if !tool_calls.is_empty() => FinishReason::ToolCalls,
            Some(reason) => reason,
            None => FinishReason::Stop,
        };
        on_event(&StreamEvent::Done { finish_reason });
        let mut message = ChatMessage::assistant(self.text);
        message.tool_calls = tool_calls;
        Ok(ChatResponse {
            message,
            finish_reason,
            usage: self.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(stream: &str) -> Vec<String> {
        let mut decoder = SseDecoder::default();
        let mut events: Vec<String> = stream
            .split('\n')
            .filter_map(|line| {
                decoder
                    .push_line(line.strip_suffix('\r').unwrap_or(line))
                    .unwrap()
            })
            .collect();
        events.extend(decoder.flush());
        events
    }

    #[test]
    fn decoder_follows_event_stream_field_rules() {
        assert_eq!(
            decode(
                ": keep-alive\nevent: message\ndata: {\"a\":1}\n\ndata:x\ndata: y\n\nid: 3\ndata: tail"
            ),
            ["{\"a\":1}", "x\ny", "tail"]
        );
        assert_eq!(decode("data: one\r\n\r\n"), ["one"]);
        assert!(decode("retry: 10\n\n").is_empty());
    }

    #[test]
    fn accumulator_assembles_text_tool_calls_and_usage() {
        let mut accumulator = StreamAccumulator::default();
        let mut events = Vec::new();
        let mut sink = |event: &StreamEvent| events.push(event.clone());
        for payload in [
            r#"{"choices":[{"index":0,"delta":{"role":"assistant","content":"Hel"}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"content":"lo"}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"read_file","arguments":""}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"a.txt\"}"}}]}}]}"#,
            r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
            r#"{"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}"#,
            "[DONE]",
        ] {
            accumulator.apply(payload, &mut sink).unwrap();
        }
        assert!(accumulator.is_done());
        let response = accumulator.finish(&mut sink).unwrap();
        assert_eq!(response.message.content, "Hello");
        assert_eq!(response.finish_reason, FinishReason::ToolCalls);
        assert_eq!(
            response.message.tool_calls,
            [ToolCall {
                provider_metadata: None,
                id: "call_a".into(),
                name: "read_file".into(),
                arguments: r#"{"path":"a.txt"}"#.into()
            }]
        );
        assert_eq!(response.usage.prompt_tokens, Some(12));
        assert!(matches!(
            events.last(),
            Some(StreamEvent::Done {
                finish_reason: FinishReason::ToolCalls
            })
        ));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, StreamEvent::ToolCallDelta { .. }))
                .count(),
            3
        );
    }

    #[test]
    fn accumulator_rejects_errors_truncation_and_nameless_calls() {
        let mut accumulator = StreamAccumulator::default();
        assert!(matches!(
            accumulator.apply(
                r#"{"error":{"code":500,"type":"server_error"}}"#,
                &mut |_| {}
            ),
            Err(ProviderError::Http { status: 500, .. })
        ));
        let mut truncated = StreamAccumulator::default();
        truncated
            .apply(
                r#"{"choices":[{"index":0,"delta":{"content":"partial"}}]}"#,
                &mut |_| {},
            )
            .unwrap();
        assert!(matches!(
            truncated.finish(&mut |_| {}),
            Err(ProviderError::Protocol(_))
        ));
        let mut nameless = StreamAccumulator::default();
        nameless
            .apply(r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]},"finish_reason":"tool_calls"}]}"#, &mut |_| {})
            .unwrap();
        assert!(matches!(
            nameless.finish(&mut |_| {}),
            Err(ProviderError::Protocol(_))
        ));
        assert!(matches!(
            StreamAccumulator::default().apply("not json", &mut |_| {}),
            Err(ProviderError::Protocol(_))
        ));
    }

    #[test]
    fn calls_without_indices_are_grouped_by_id() {
        let mut accumulator = StreamAccumulator::default();
        for payload in [
            r#"{"choices":[{"delta":{"tool_calls":[{"id":"a","function":{"name":"one","arguments":"{"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"function":{"arguments":"}"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"id":"b","function":{"name":"two","arguments":"{}"}}]}}]}"#,
            "[DONE]",
        ] {
            accumulator.apply(payload, &mut |_| {}).unwrap();
        }
        let calls = accumulator.finish(&mut |_| {}).unwrap().message.tool_calls;
        assert_eq!(calls.len(), 2);
        assert_eq!(
            (calls[0].arguments.as_str(), calls[1].name.as_str()),
            ("{}", "two")
        );
    }
}
