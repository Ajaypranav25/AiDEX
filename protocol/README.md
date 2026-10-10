# Protocol

This area holds language-neutral wire contracts and schemas. `schemas/json-rpc-2.0.schema.json` defines the JSON-RPC 2.0 envelope only; method names, authorization, transport, and method-specific payloads remain undefined until the owning services establish them. It is an envelope schema, not a complete AiDEX service API; method-specific contracts should be added here when defined.