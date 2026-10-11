//! Validated provider and model identifiers.
//!
//! `aidex_core::ModelReference` uses `EntityId` for both halves, but upstream model
//! names routinely contain `/` (`meta-llama/Llama-3.1-8B`), which `EntityId` rejects,
//! so [`ModelRef`] pairs an `EntityId`-backed [`ProviderId`] with a wider [`ModelId`].

use std::{fmt, str::FromStr};

use aidex_core::EntityId;
use serde::{Deserialize, Serialize};

const MAX_PROVIDER_ID: usize = 64;
const MAX_MODEL_ID: usize = 256;

/// An identifier was rejected; the message names the rule, never echoes the input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidId {
    what: &'static str,
    rule: &'static str,
}

impl InvalidId {
    pub(crate) const fn new(what: &'static str, rule: &'static str) -> Self {
        Self { what, rule }
    }
}

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.what, self.rule)
    }
}

impl std::error::Error for InvalidId {}

/// A provider identifier: 1-64 ASCII lowercase letters, digits, `-`, or `_`,
/// starting with a letter or digit (for example `openai` or `local`).
///
/// It names the vault consumer scope `provider:<id>` and is the part before the
/// first `/` in `provider/model` references, so `/`, `:`, and `.` are excluded.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProviderId(EntityId);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidId> {
        const RULE: &str =
            "use 1-64 lowercase ASCII letters, digits, `-` or `_`, starting with a letter or digit";
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_PROVIDER_ID
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
            && value
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphanumeric());
        if !valid {
            return Err(InvalidId::new("provider id", RULE));
        }
        EntityId::new(value)
            .map(Self)
            .map_err(|_| InvalidId::new("provider id", RULE))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn as_entity_id(&self) -> &EntityId {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<String> for ProviderId {
    type Error = InvalidId;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ProviderId> for String {
    fn from(id: ProviderId) -> Self {
        id.0.as_str().to_owned()
    }
}

impl FromStr for ProviderId {
    type Err = InvalidId;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// A model name as the provider serves it: 1-256 ASCII letters, digits, or
/// `.`, `_`, `-`, `:`, `/`, `@`, `+` (for example `gpt-4o-mini`,
/// `meta-llama/Llama-3.1-8B-Instruct`, or `llama3.1:8b`).
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ModelId(String);

impl ModelId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidId> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= MAX_MODEL_ID
            && value.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(b, b'.' | b'_' | b'-' | b':' | b'/' | b'@' | b'+')
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(InvalidId::new(
                "model id",
                "use 1-256 ASCII letters, digits, or . _ - : / @ +",
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ModelId {
    type Error = InvalidId;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ModelId> for String {
    fn from(id: ModelId) -> Self {
        id.0
    }
}

impl FromStr for ModelId {
    type Err = InvalidId;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// A model served by a specific provider, written `provider/model` in text form.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub provider_id: ProviderId,
    pub model_id: ModelId,
}

impl ModelRef {
    pub fn new(provider_id: ProviderId, model_id: ModelId) -> Self {
        Self {
            provider_id,
            model_id,
        }
    }

    /// Parse `provider/model`, splitting at the first `/`.
    pub fn parse(text: &str) -> Result<Self, InvalidId> {
        let (provider, model) = text.split_once('/').ok_or(InvalidId::new(
            "model reference",
            "expected PROVIDER/MODEL, for example openai/gpt-4o-mini",
        ))?;
        Ok(Self::new(ProviderId::new(provider)?, ModelId::new(model)?))
    }
}

impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.provider_id, self.model_id)
    }
}

impl FromStr for ModelRef {
    type Err = InvalidId;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_are_narrow_and_model_ids_allow_upstream_names() {
        for valid in ["openai", "local", "my-server_2", "0x"] {
            assert!(ProviderId::new(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "OpenAI",
            "a/b",
            "a:b",
            "a.b",
            "-a",
            "_a",
            &"a".repeat(65),
        ] {
            assert!(ProviderId::new(invalid).is_err(), "{invalid}");
        }
        for valid in [
            "gpt-4o-mini",
            "meta-llama/Llama-3.1-8B-Instruct",
            "llama3.1:8b",
            "openai/gpt-4o:free",
            "qwen2.5-coder-0.5b-instruct-q4_k_m.gguf",
        ] {
            assert!(ModelId::new(valid).is_ok(), "{valid}");
        }
        for invalid in ["", "a b", "a\\b", "a\"b", "ü", &"m".repeat(257)] {
            assert!(ModelId::new(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn model_refs_split_at_the_first_slash_and_round_trip() {
        let model = ModelRef::parse("vllm/meta-llama/Llama-3.1-8B").unwrap();
        assert_eq!(model.provider_id.as_str(), "vllm");
        assert_eq!(model.model_id.as_str(), "meta-llama/Llama-3.1-8B");
        assert_eq!(model.to_string(), "vllm/meta-llama/Llama-3.1-8B");
        assert!(ModelRef::parse("no-slash").is_err());
        assert!(ModelRef::parse("/model").is_err());
        let json = serde_json::to_value(&model).unwrap();
        assert_eq!(json["provider_id"], "vllm");
        assert_eq!(serde_json::from_value::<ModelRef>(json).unwrap(), model);
        assert!(serde_json::from_str::<ProviderId>("\"Bad\"").is_err());
    }
}
