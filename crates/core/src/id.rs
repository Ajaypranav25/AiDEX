//! Opaque entity identifiers and time-ordered identifier generation.

use std::{
    error::Error,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Maximum identifier length in bytes.
const MAX_ID_LEN: usize = 128;
/// Maximum generated-identifier prefix length.
const MAX_PREFIX_LEN: usize = 16;
/// Crockford base32 alphabet; ASCII order matches numeric order.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Largest timestamp a 48-bit ULID time field can carry.
const MAX_ULID_MILLIS: u64 = (1 << 48) - 1;

/// An opaque, caller-assigned identifier safe to use as a logical key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdError {
    Empty,
    TooLong,
    InvalidCharacter(char),
    /// A generated-identifier prefix was not 1-16 ASCII lowercase letters.
    InvalidPrefix,
    /// The operating-system random number generator failed; no ID was produced.
    Randomness(String),
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("identifier must not be empty"),
            Self::TooLong => f.write_str("identifier must be at most 128 bytes"),
            Self::InvalidCharacter(ch) => write!(f, "invalid identifier character: {ch:?}"),
            Self::InvalidPrefix => {
                f.write_str("identifier prefix must be 1-16 ASCII lowercase letters")
            }
            Self::Randomness(reason) => {
                write!(f, "operating-system randomness is unavailable: {reason}")
            }
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
        if value.len() > MAX_ID_LEN {
            return Err(IdError::TooLong);
        }
        for ch in value.chars() {
            if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':')) {
                return Err(IdError::InvalidCharacter(ch));
            }
        }
        Ok(Self(value))
    }

    /// Generate `<prefix>_<ULID>`: a 48-bit Unix-millisecond timestamp followed by
    /// 80 bits from the operating-system RNG, encoded as 26 Crockford base32 characters.
    ///
    /// IDs sort lexically by creation millisecond for a given prefix; IDs created in
    /// the same millisecond are unique with overwhelming probability but unordered.
    /// RNG failure is reported as [`IdError::Randomness`]; weaker randomness is never
    /// substituted. A system clock before 1970 encodes time zero.
    pub fn generate(prefix: &str) -> Result<Self, IdError> {
        if prefix.is_empty()
            || prefix.len() > MAX_PREFIX_LEN
            || !prefix.bytes().all(|byte| byte.is_ascii_lowercase())
        {
            return Err(IdError::InvalidPrefix);
        }
        let mut random = [0_u8; 10];
        getrandom::fill(&mut random).map_err(|error| IdError::Randomness(error.to_string()))?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_millis()).unwrap_or(MAX_ULID_MILLIS)
            });
        Ok(Self(format!("{prefix}_{}", encode_ulid(millis, random))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Encode a ULID as 26 Crockford base32 characters (two leading pad bits).
fn encode_ulid(millis: u64, random: [u8; 10]) -> String {
    let value = random
        .iter()
        .fold(u128::from(millis.min(MAX_ULID_MILLIS)), |value, &byte| {
            (value << 8) | u128::from(byte)
        });
    (0..26)
        .map(|index| {
            let shift = 125 - 5 * index;
            char::from(CROCKFORD[((value >> shift) & 0x1f) as usize])
        })
        .collect()
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

impl Serialize for EntityId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl JsonSchema for EntityId {
    fn schema_name() -> String {
        "EntityId".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}
