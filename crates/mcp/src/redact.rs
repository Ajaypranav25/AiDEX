//! Redaction helpers: credential patterns, known secret values, and size bounds.

use std::fmt;

use zeroize::Zeroizing;

/// Cut `text` to at most `max` bytes on a character boundary.
pub(crate) fn truncate_utf8(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

/// `text` with credential patterns redacted, cut to `max` bytes (marked `...`).
pub(crate) fn bounded_redacted(text: &str, max: usize) -> String {
    let redacted = aidex_events::redact_text(text);
    let (head, cut) = truncate_utf8(&redacted, max);
    if cut {
        format!("{head}...")
    } else {
        head.to_owned()
    }
}

/// Replaces the resolved secret values of one server with `[REDACTED]` in
/// everything that server sends back (protocol frames and stderr).
#[derive(Clone, Default)]
pub struct Scrubber {
    secrets: Vec<Zeroizing<String>>,
}

impl Scrubber {
    pub fn new(secrets: Vec<Zeroizing<String>>) -> Self {
        Self {
            secrets: secrets
                .into_iter()
                .filter(|secret| secret.len() >= 4)
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// `text` without any known secret value.
    pub fn text(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for secret in &self.secrets {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), "[REDACTED]");
            }
        }
        out
    }

    /// `value` with every string (keys included) scrubbed.
    pub fn value(&self, value: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        if self.secrets.is_empty() {
            return value;
        }
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            Value::Array(items) => Value::Array(items.into_iter().map(|v| self.value(v)).collect()),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, v)| (self.text(&key), self.value(v)))
                    .collect(),
            ),
            other => other,
        }
    }
}

impl fmt::Debug for Scrubber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Scrubber({} secrets, values redacted)",
            self.secrets.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubber_removes_known_values_everywhere() {
        let scrubber = Scrubber::new(vec![Zeroizing::new("opaque-value-1234".into())]);
        let value = serde_json::json!({"a": ["x opaque-value-1234 y"], "opaque-value-1234": 1});
        let text = scrubber.value(value).to_string();
        assert!(!text.contains("opaque-value-1234"));
        assert!(text.contains("[REDACTED]"));
        assert!(!format!("{scrubber:?}").contains("opaque"));
    }

    #[test]
    fn truncation_respects_character_boundaries() {
        assert_eq!(truncate_utf8("héllo", 2), ("h", true));
        assert_eq!(truncate_utf8("abc", 5), ("abc", false));
        assert_eq!(bounded_redacted("abcdef", 3), "abc...");
    }
}
