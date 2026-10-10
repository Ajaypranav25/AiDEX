use aidex_core::{ContractPayload, Versioned};
use std::path::PathBuf;

fn main() {
    let schema = schemars::schema_for!(Versioned<ContractPayload>);
    let output = format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("serialize schema")
    );
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../protocol/schemas/contract-v1.schema.json");
    std::fs::write(path, output).expect("write contract schema");
}
