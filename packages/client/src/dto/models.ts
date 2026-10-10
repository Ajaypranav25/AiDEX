/** Mirrors aidex-providers and aidexd::methods::models; never contains key values. */
export type ProviderKind =
  | "openai-compatible"
  | "openai-responses"
  | "anthropic"
  | "gemini"
  | "azure-openai"
  | "bedrock"
  | "llama-cpp";
export type ModelRef = { provider_id: string; model_id: string };
export type ProviderPreset = {
  id: string;
  display_name: string;
  kind: ProviderKind;
  base_url: string | null;
  discover_models: boolean;
  tool_calls: boolean;
  json_schema_output: boolean;
  authentication: string;
  oauth_supported: boolean;
};
export type ProviderAccount = {
  descriptor: {
    id: string;
    kind: ProviderKind;
    base_url: string | null;
    locality: "local" | "remote";
    auth: { type: "none" } | { type: "vault_secret"; secret_name: string };
  };
  origin: "configured" | "runtime";
};
export type ProviderModel = {
  model: ModelRef;
  display_name: string;
  locality: "local" | "remote";
  context_window: number;
  max_output_tokens: number | null;
  capabilities: {
    chat: boolean;
    streaming: boolean;
    tool_calls: "native" | "structured" | "none";
    json_schema_output: boolean;
    vision: boolean;
    embeddings: boolean;
  };
  roles: string[];
  source: string;
};
export type ModelList = {
  models: ProviderModel[];
  failures: { provider_id: string; kind: string; message: string }[];
  default_model: ModelRef | null;
};
export type ProviderAddParams = {
  id: string;
  kind: ProviderKind;
  base_url: string;
  secret_name?: string;
  discover_models: boolean;
  models?: {
    id: string;
    context_window: number;
    tool_calls?: "native" | "none";
  }[];
};
