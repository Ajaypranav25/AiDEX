//! Schema generation and strict, stable-error input/output validation.
//!
//! Input and output types are Rust structs with `#[serde(deny_unknown_fields)]`;
//! their JSON Schemas are generated from the same types, so the published schema
//! and the validator cannot disagree.

use schemars::{JsonSchema, r#gen::SchemaSettings};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{InputError, ToolError};

/// The JSON Schema of `T`: draft 7, subschemas inlined, no `$schema` key.
pub fn schema_of<T: JsonSchema>() -> Value {
    let generator = SchemaSettings::draft07()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.meta_schema = None;
        })
        .into_generator();
    let root = generator.into_root_schema_for::<T>();
    serde_json::to_value(root).unwrap_or(Value::Null)
}

/// Parse a tool input into its typed struct, mapping serde failures to
/// [`ToolError::InvalidInput`] with a field path and a stable [`InputError`].
pub fn parse_input<T: DeserializeOwned>(input: Value) -> Result<T, ToolError> {
    if !input.is_object() {
        return Err(ToolError::invalid(
            "",
            InputError::NotAnObject,
            "the input must be a JSON object",
        ));
    }
    serde_path_to_error::deserialize::<_, T>(input).map_err(|error| {
        let path = error.path().to_string();
        let message = error.inner().to_string();
        let (reason, field) = classify(&message);
        let base = if path == "." { String::new() } else { path };
        let field = match field {
            // Unknown keys are already the last path segment.
            Some(name) if base == name || base.ends_with(&format!(".{name}")) => base,
            Some(name) if base.is_empty() => name,
            Some(name) => format!("{base}.{name}"),
            None => base,
        };
        ToolError::invalid(field, reason, message)
    })
}

/// Classify a serde message; returns the reason and, for unknown/missing fields, the
/// field name (serde reports it in the message rather than the path).
fn classify(message: &str) -> (InputError, Option<String>) {
    let quoted = |rest: &str| {
        rest.strip_prefix('`')
            .and_then(|tail| tail.split_once('`'))
            .map(|(name, _)| name.to_owned())
    };
    if let Some(rest) = message.strip_prefix("unknown field ") {
        (InputError::UnknownField, quoted(rest))
    } else if let Some(rest) = message.strip_prefix("missing field ") {
        (InputError::MissingField, quoted(rest))
    } else if message.starts_with("invalid type") {
        (InputError::InvalidType, None)
    } else if message.starts_with("invalid length") {
        (InputError::OutOfRange, None)
    } else {
        (InputError::InvalidValue, None)
    }
}

/// Serialize a typed output and check that it round-trips through its own type, so
/// every result conforms to the published output schema.
pub fn output_value<T: Serialize + DeserializeOwned>(output: &T) -> Result<Value, ToolError> {
    let value = serde_json::to_value(output)
        .map_err(|error| ToolError::InvalidOutput(error.to_string()))?;
    serde_json::from_value::<T>(value.clone())
        .map_err(|error| ToolError::InvalidOutput(error.to_string()))?;
    Ok(value)
}

/// Serialize a validated input back to its normalized JSON form (defaults filled in).
pub(crate) fn normalized<T: Serialize>(input: &T) -> Result<Value, ToolError> {
    serde_json::to_value(input).map_err(|error| ToolError::InvalidOutput(error.to_string()))
}
