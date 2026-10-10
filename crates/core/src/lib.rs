//! Small, dependency-free primitives shared across AiDEX Rust domains.

use std::{error::Error, fmt};

/// An opaque, caller-assigned identifier safe to use as a logical key.
///
/// IDs are deliberately not generated here: generation policy belongs to the
/// domain creating the entity, while this type enforces a stable wire-safe form.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdError {
    Empty,
    TooLong,
    InvalidCharacter(char),
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("identifier must not be empty"),
            Self::TooLong => f.write_str("identifier must be at most 128 bytes"),
            Self::InvalidCharacter(ch) => write!(f, "invalid identifier character: {ch:?}"),
        }
    }
}

impl Error for IdError {}

impl EntityId {
    /// Construct an ID from ASCII letters, digits, `-`, `_`, `.`, or `:`.
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(IdError::Empty);
        }
        if value.len() > 128 {
            return Err(IdError::TooLong);
        }
        for ch in value.chars() {
            if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':')) {
                return Err(IdError::InvalidCharacter(ch));
            }
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for EntityId {
    type Error = IdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for EntityId {
    type Error = IdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
