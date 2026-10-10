//! One line per event: human text or a JSON envelope.

use aidex_events::EventEnvelope;
use serde_json::Value;
use std::io::{self, Write};

/// `cursor  time  name  scope  data`. Ephemeral events show `~cursor` (the durable
/// event they follow) because they are live-only and never replayed.
pub(crate) fn human_line(event: &EventEnvelope) -> String {
    let cursor = if event.is_durable() {
        event.cursor.to_string()
    } else {
        format!("~{}", event.cursor)
    };
    let scope = match (&event.scope.workspace_id, &event.scope.session_id) {
        (Some(workspace), Some(session)) => format!("{workspace}/{session}"),
        (Some(workspace), None) => workspace.to_string(),
        (None, Some(session)) => format!("?/{session}"),
        (None, None) => "global".to_owned(),
    };
    let data = serde_json::to_value(&event.event)
        .ok()
        .and_then(|mut value| value.get_mut("data").map(Value::take))
        .filter(|data| data.as_object().is_none_or(|fields| !fields.is_empty()))
        .map_or_else(String::new, |data| data.to_string());
    format!(
        "{cursor:>7}  {}  {:<28} {scope}  {data}",
        event.occurred_at.as_str(),
        event.name()
    )
    .trim_end()
    .to_owned()
}

/// Write one event line and flush it, so a consumer has it before we acknowledge it.
pub(crate) fn write_event<W: Write>(
    output: &mut W,
    event: &EventEnvelope,
    json: bool,
) -> io::Result<()> {
    if json {
        serde_json::to_writer(&mut *output, event).map_err(io::Error::other)?;
        output.write_all(b"\n")?;
    } else {
        writeln!(output, "{}", human_line(event))?;
    }
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn envelope(kind: Value, scope: Value) -> EventEnvelope {
        serde_json::from_value(json!({
            "version": 2,
            "cursor": 12,
            "scope": scope,
            "occurred_at": "2026-10-10T07:51:01.042Z",
            "event": kind,
        }))
        .unwrap()
    }

    #[test]
    fn human_lines_show_cursor_time_name_scope_and_data() {
        let task = envelope(
            json!({"type": "task.started", "data": {"task_id": "t1"}}),
            json!({"workspace_id": "ws", "session_id": "ses"}),
        );
        assert_eq!(
            human_line(&task),
            format!(
                "     12  2026-10-10T07:51:01.042Z  {:<28} ws/ses  {{\"task_id\":\"t1\"}}",
                "task.started"
            )
        );
        let delta = envelope(
            json!({"type": "agent.delta", "data": {"text": "hi"}}),
            json!({}),
        );
        assert!(
            human_line(&delta).starts_with("    ~12"),
            "{}",
            human_line(&delta)
        );
        let empty = envelope(
            json!({"type": "drive.safe_to_remove", "data": {}}),
            json!({}),
        );
        assert!(
            human_line(&empty).ends_with("global"),
            "{}",
            human_line(&empty)
        );
    }

    #[test]
    fn json_lines_are_envelopes() {
        let event = envelope(
            json!({"type": "task.started", "data": {"task_id": "t1"}}),
            json!({}),
        );
        let mut buffer = Vec::new();
        write_event(&mut buffer, &event, true).unwrap();
        let line = String::from_utf8(buffer).unwrap();
        assert!(line.ends_with('\n'));
        assert_eq!(EventEnvelope::decode(line.trim_end()).unwrap(), event);
    }
}
