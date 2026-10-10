//! Contract version values, negotiation, and the versioned payload envelope.

use std::{error::Error, fmt};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::ServiceErrorCode;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ContractVersion(u16);

impl JsonSchema for ContractVersion {
    fn schema_name() -> String {
        "ContractVersion".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = u16::json_schema(generator);
        if let Schema::Object(object) = &mut schema {
            object.number().minimum = Some(1.0);
        }
        schema
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractVersionError;

impl fmt::Display for ContractVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("contract version must be at least 1")
    }
}

impl Error for ContractVersionError {}

impl ContractVersion {
    pub const CURRENT: Self = Self(1);

    pub fn new(value: u16) -> Result<Self, ContractVersionError> {
        Self::try_from(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for ContractVersion {
    type Error = ContractVersionError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value == 0 {
            Err(ContractVersionError)
        } else {
            Ok(Self(value))
        }
    }
}

impl<'de> Deserialize<'de> for ContractVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(u16::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractNegotiationError {
    InvalidRange,
    IncompatibleVersion,
}

impl fmt::Display for ContractNegotiationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange => f.write_str("contract offer minimum exceeds maximum"),
            Self::IncompatibleVersion => f.write_str("no compatible contract version"),
        }
    }
}

impl Error for ContractNegotiationError {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Versioned<T> {
    pub version: ContractVersion,
    pub payload: T,
}

impl<T> Versioned<T> {
    pub fn new(payload: T) -> Self {
        Self {
            version: ContractVersion::CURRENT,
            payload,
        }
    }
    pub fn validate_version(&self) -> Result<(), ServiceErrorCode> {
        if self.version == ContractVersion::CURRENT {
            Ok(())
        } else {
            Err(ServiceErrorCode::IncompatibleVersion)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContractOffer {
    pub minimum: ContractVersion,
    pub maximum: ContractVersion,
}

impl ContractOffer {
    pub fn negotiate(&self, peer: &Self) -> Result<ContractVersion, ContractNegotiationError> {
        if self.minimum > self.maximum || peer.minimum > peer.maximum {
            return Err(ContractNegotiationError::InvalidRange);
        }
        let minimum = self.minimum.max(peer.minimum);
        let maximum = self.maximum.min(peer.maximum);
        if minimum > maximum {
            Err(ContractNegotiationError::IncompatibleVersion)
        } else {
            Ok(maximum)
        }
    }
}
