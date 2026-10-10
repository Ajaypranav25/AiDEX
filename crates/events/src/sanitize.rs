//! Append-time validation: scope rules, redaction of every text field, size bounds.

use std::borrow::Cow;

use serde_json::Value;

use crate::{EventEnvelope, EventError, EventKind, EventScope, redact};

/// Largest encoded envelope accepted, in bytes. Every text field is already bounded
/// to [`redact::MAX_TEXT_BYTES`]; this also bounds lists such as `diff.created.files`
/// and keeps one event well inside the daemon's 1 MiB IPC message limit.
pub const MAX_EVENT_BYTES: usize = 256 * 1024;

/// Validate the scope and redact/bound the payload. Free text is rewritten in place;
/// an identifier that looks like a credential refuses the whole event, because an
/// identifier cannot be rewritten without changing what it refers to.
pub(crate) fn prepare(scope: &EventScope, event: EventKind) -> Result<EventKind, EventError> {
    scope.validate()?;
    let scope_ids = [&scope.workspace_id, &scope.session_id];
    if scope_ids
        .into_iter()
        .flatten()
        .any(|id| matches!(redact::redact_text(id.as_str()), Cow::Owned(_)))
    {
        return Err(EventError::SensitiveIdentifier);
    }
    let mut value =
        serde_json::to_value(&event).map_err(|error| EventError::Decode(error.to_string()))?;
    if !sanitize_strings(&mut value) {
        return Ok(event);
    }
    // Free text (and workspace paths) accept the redaction marker; a redacted
    // identifier no longer parses, so the event is refused.
    serde_json::from_value(value).map_err(|_| EventError::SensitiveIdentifier)
}

/// Encode for storage or delivery, enforcing [`MAX_EVENT_BYTES`].
pub(crate) fn encode(envelope: &EventEnvelope) -> Result<String, EventError> {
    let body =
        serde_json::to_string(envelope).map_err(|error| EventError::Decode(error.to_string()))?;
    if body.len() > MAX_EVENT_BYTES {
        return Err(EventError::TooLarge {
            bytes: body.len(),
            limit: MAX_EVENT_BYTES,
        });
    }
    Ok(body)
}

/// Redact and bound every string; returns whether anything changed.
fn sanitize_strings(value: &mut Value) -> bool {
    match value {
        Value::String(text) => match redact::sanitize_text(text) {
            Cow::Borrowed(_) => false,
            Cow::Owned(clean) => {
                *text = clean;
                true
            }
        },
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |changed, item| sanitize_strings(item) | changed),
        Value::Object(fields) => fields
            .values_mut()
            .fold(false, |changed, item| sanitize_strings(item) | changed),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

/// Redacted, bounded JSON of an event's data, for `Debug`.
pub(crate) fn debug_data(event: &EventKind) -> String {
    match serde_json::to_value(event) {
        Ok(mut value) => {
            sanitize_strings(&mut value);
            value
                .get("data")
                .map_or_else(|| "{}".to_owned(), Value::to_string)
        }
        Err(_) => "{..}".to_owned(),
    }
}
