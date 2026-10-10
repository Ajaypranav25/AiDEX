//! SHA-256 plan fingerprints over canonical JSON.

use std::fmt;

use serde::{Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Domain separation for plan fingerprints; bump when the hashed layout changes.
pub(crate) const FINGERPRINT_DOMAIN: &str = "aidex-tools.plan.v1";

/// SHA-256 over a plan's canonical JSON (tool, version, ids, root, normalized input,
/// facts, preview, risk, summary, and tool state). Approvals and permits bind to it.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct PlanFingerprint([u8; 32]);

impl PlanFingerprint {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        crate::fs::hex(&self.0)
    }

    /// Parse 64 lowercase or uppercase hexadecimal characters.
    pub fn from_hex(text: &str) -> Option<Self> {
        if text.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
        }
        Some(Self(bytes))
    }

    /// Fingerprint of a JSON value in canonical form.
    pub(crate) fn of(value: &Value) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(FINGERPRINT_DOMAIN.as_bytes());
        hasher.update([0]);
        canonical(value, &mut hasher);
        Self(hasher.finalize().into())
    }
}

impl fmt::Display for PlanFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for PlanFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PlanFingerprint({})", self.to_hex())
    }
}

impl Serialize for PlanFingerprint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

/// Feed `value` to the hasher as canonical JSON: object keys sorted by their UTF-8
/// bytes, no insignificant whitespace, strings escaped by `serde_json`.
fn canonical(value: &Value, hasher: &mut Sha256) {
    match value {
        Value::Null => hasher.update(b"null"),
        Value::Bool(true) => hasher.update(b"true"),
        Value::Bool(false) => hasher.update(b"false"),
        Value::Number(number) => hasher.update(number.to_string().as_bytes()),
        Value::String(text) => string(text, hasher),
        Value::Array(items) => {
            hasher.update(b"[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    hasher.update(b",");
                }
                canonical(item, hasher);
            }
            hasher.update(b"]");
        }
        Value::Object(fields) => {
            let mut keys: Vec<&String> = fields.keys().collect();
            keys.sort();
            hasher.update(b"{");
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    hasher.update(b",");
                }
                string(key, hasher);
                hasher.update(b":");
                if let Some(item) = fields.get(key) {
                    canonical(item, hasher);
                }
            }
            hasher.update(b"}");
        }
    }
}

fn string(text: &str, hasher: &mut Sha256) {
    match serde_json::to_string(text) {
        Ok(escaped) => hasher.update(escaped.as_bytes()),
        // Serializing a &str cannot fail; hash the raw bytes length-prefixed otherwise.
        Err(_) => {
            hasher.update((text.len() as u64).to_le_bytes());
            hasher.update(text.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn key_order_does_not_change_the_fingerprint() {
        let a: Value = serde_json::from_str(r#"{"b":1,"a":{"y":[1,2],"x":"s"}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":{"x":"s","y":[1,2]},"b":1}"#).unwrap();
        assert_eq!(PlanFingerprint::of(&a), PlanFingerprint::of(&b));
    }

    #[test]
    fn any_change_changes_the_fingerprint() {
        let base = PlanFingerprint::of(&json!({"path": "a.txt", "content": "x"}));
        assert_ne!(
            base,
            PlanFingerprint::of(&json!({"path": "a.txt", "content": "y"}))
        );
        assert_ne!(
            base,
            PlanFingerprint::of(&json!({"path": "b.txt", "content": "x"}))
        );
        assert_ne!(
            PlanFingerprint::of(&json!(["a", "b"])),
            PlanFingerprint::of(&json!(["b", "a"]))
        );
    }

    #[test]
    fn hex_round_trips() {
        let fingerprint = PlanFingerprint::of(&json!({"k": "v"}));
        let hex = fingerprint.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(PlanFingerprint::from_hex(&hex), Some(fingerprint));
        assert_eq!(PlanFingerprint::from_hex("zz"), None);
    }
}
