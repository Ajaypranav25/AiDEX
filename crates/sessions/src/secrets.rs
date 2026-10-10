//! Defence-in-depth credential redaction for persisted session records.
//!
//! Session records must never hold secret values: credentials are passed as vault
//! handles or [`aidex_core::SecretRef`]s, never as text. As a last line of defence,
//! every free-text field (titles, messages, tool inputs and summaries, failure
//! reasons, checkpoint payloads, text artifacts) passes through
//! [`aidex_events::redact_text`] before it is written. Redaction is pattern based and
//! cannot recognise arbitrary secrets, so it does not replace handle discipline.

use std::borrow::Cow;

use aidex_events::redact_text;
use serde_json::{Map, Value};

/// Placeholder written where a credential was removed.
pub use aidex_events::redact::REDACTED;

/// Whether `text` contains something the redactor recognises as a credential.
pub fn contains_secret_like(text: &str) -> bool {
    matches!(redact_text(text), Cow::Owned(_))
}

/// Redact free text; the flag reports whether anything was replaced.
pub(crate) fn redact(text: &str) -> (String, bool) {
    match redact_text(text) {
        Cow::Borrowed(clean) => (clean.to_owned(), false),
        Cow::Owned(redacted) => (redacted, true),
    }
}

/// Redact every string (and object key) of a JSON document.
///
/// The document is first redacted as one JSON text so `"password": "..."`-style
/// assignments are seen with their key. If that rewrite does not parse back (a
/// replaced span crossed a JSON delimiter), each string is redacted together with
/// its key instead, and a value that still cannot be rewritten safely is replaced
/// by [`REDACTED`] entirely (fail closed).
pub(crate) fn redact_json(value: Value) -> (Value, bool) {
    let Ok(text) = serde_json::to_string(&value) else {
        return (Value::String(REDACTED.into()), true);
    };
    match redact_text(&text) {
        Cow::Borrowed(_) => (value, false),
        Cow::Owned(redacted) => match serde_json::from_str(&redacted) {
            Ok(parsed) => (parsed, true),
            Err(_) => (redact_tree(value, None), true),
        },
    }
}

fn redact_tree(value: Value, key: Option<&str>) -> Value {
    match value {
        Value::String(text) => Value::String(redact_leaf(key, text)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redact_tree(item, None))
                .collect(),
        ),
        Value::Object(entries) => {
            let mut out = Map::new();
            for (name, item) in entries {
                let clean_name = redact(&name).0;
                let item = redact_tree(item, Some(&name));
                out.insert(clean_name, item);
            }
            Value::Object(out)
        }
        other => other,
    }
}

/// Redact one string in the context of its key, as a single-entry JSON object.
fn redact_leaf(key: Option<&str>, text: String) -> String {
    let Some(key) = key else {
        return redact(&text).0;
    };
    let mut single = Map::new();
    single.insert(key.to_owned(), Value::String(text));
    let Ok(encoded) = serde_json::to_string(&Value::Object(single)) else {
        return REDACTED.into();
    };
    match redact_text(&encoded) {
        Cow::Borrowed(_) => match serde_json::from_str::<Map<String, Value>>(&encoded) {
            Ok(mut map) => match map.remove(key) {
                Some(Value::String(text)) => text,
                _ => REDACTED.into(),
            },
            Err(_) => REDACTED.into(),
        },
        Cow::Owned(redacted) => match serde_json::from_str::<Map<String, Value>>(&redacted) {
            Ok(map) => match map.into_iter().next() {
                Some((_, Value::String(text))) => text,
                _ => REDACTED.into(),
            },
            Err(_) => REDACTED.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

    #[test]
    fn secret_like_text_is_detected() {
        assert!(contains_secret_like(&format!("push with {TOKEN}")));
        assert!(contains_secret_like(
            "Authorization: Bearer abc.def.ghijklmnop"
        ));
        assert!(contains_secret_like("password=hunter22"));
        assert!(!contains_secret_like("read src/main.rs and summarise it"));
    }

    #[test]
    fn json_values_are_redacted_with_their_keys() {
        let (clean, changed) = redact_json(json!({
            "command": format!("git push https://{TOKEN}@github.com/o/r"),
            "password": "hunter22",
            "nested": [{"api_key": "AbCdEf0123456789XyZw"}],
            "path": "src/lib.rs",
            "count": 3,
        }));
        assert!(changed);
        let text = clean.to_string();
        assert!(!text.contains(TOKEN), "{text}");
        assert!(!text.contains("hunter22"), "{text}");
        assert!(!text.contains("AbCdEf0123456789XyZw"), "{text}");
        assert_eq!(clean["path"], "src/lib.rs");
        assert_eq!(clean["count"], 3);
        let (same, changed) = redact_json(json!({"path": "README.md"}));
        assert!(!changed);
        assert_eq!(same, json!({"path": "README.md"}));
    }

    #[test]
    fn tree_fallback_redacts_each_leaf_in_context() {
        let value = redact_tree(
            json!({"token": "AbCdEf0123456789XyZw", "list": [TOKEN, "ok"]}),
            None,
        );
        let text = value.to_string();
        assert!(!text.contains("AbCdEf0123456789XyZw"), "{text}");
        assert!(!text.contains(TOKEN), "{text}");
        assert_eq!(value["list"][1], "ok");
    }
}
