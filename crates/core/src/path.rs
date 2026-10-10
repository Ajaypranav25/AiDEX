//! Workspace-relative path values carried across service contracts.

use std::{error::Error, fmt};

use schemars::{JsonSchema, r#gen::SchemaGenerator, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// A normalized relative path using `/` separators, never an absolute path.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WorkspacePath(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspacePathError;

impl fmt::Display for WorkspacePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("path must be a non-empty workspace-relative path without traversal")
    }
}
impl Error for WorkspacePathError {}

impl WorkspacePath {
    pub fn new(value: impl Into<String>) -> Result<Self, WorkspacePathError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 4096
            || value.starts_with('/')
            || value.contains('\\')
            || value.contains(':')
            || value.chars().any(char::is_control)
            || value
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(WorkspacePathError);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl Serialize for WorkspacePath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for WorkspacePath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(de::Error::custom)
    }
}
impl JsonSchema for WorkspacePath {
    fn schema_name() -> String {
        "WorkspacePath".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        String::json_schema(generator)
    }
}
