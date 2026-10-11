# aidex-providers

Model providers for AiDEX: provider-neutral chat contracts, the synchronous `ModelProvider` trait, a deterministic `FakeProvider`, protocol-specific OpenAI Responses, Chat Completions, Anthropic Messages, Gemini, Azure OpenAI, and AWS Bedrock ConverseStream adapters, request-time vault credentials, the offline egress gate, and the `ProviderRegistry`. Depends only on `aidex-core` and `aidex-drive` (plus `ureq` with rustls, `toml`, `zeroize`, `hmac`, `sha2`, `crc32fast`).

Routing between models lives in [`aidex-router`](../router/README.md); the managed local llama.cpp runtime (issue #15) implements `ModelProvider` in `aidex-inference`.

## API

| Area           | Items                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Identifiers    | `ProviderId` (1-64 `[a-z0-9_-]`, `EntityId`-backed; names the vault consumer `provider:<id>`), `ModelId` (1-256 ASCII letters/digits and `. _ - : / @ +`, so upstream names like `meta-llama/Llama-3.1-8B` work), `ModelRef { provider_id, model_id }` written `provider/model` (split at the first `/`). `aidex_core::ModelReference` is not reused because its `EntityId` model half rejects `/`.                                                                                                                                                                                                                                                                                                                             |
| Models         | `ModelDescriptor { model, display_name, locality: Local\|Remote, context_window, max_output_tokens, capabilities: ModelCapabilities { chat, streaming, tool_calls: Native\|Structured\|None, json_schema_output, vision, embeddings }, roles: [Code\|Plan\|Summarize\|Classify\|Embed\|Verify\|General], cost: Option<CostHint>, min_ram_bytes, source: Configured\|Discovered\|Assumed }`                                                                                                                                                                                                                                                                                                                                      |
| Chat           | `ChatRequest { model: ModelId, messages: [ChatMessage { role: System\|User\|Assistant\|Tool, content, tool_calls: [ToolCall { id, name, arguments, provider_metadata? }], tool_call_id }], tools: [ToolSpec { name, description, parameters }], tool_choice: Auto\|None\|Required\|Tool{name}, response_format: Option<ResponseFormat::JsonSchema { name, schema, strict }>, temperature, max_tokens, stop }` with `validate()` and `estimated_input_tokens()`; `StreamEvent::{TextDelta{text}, ToolCallDelta{index, id, name, arguments}, Usage(Usage), Done{finish_reason}}`; `ChatResponse { message, finish_reason: Stop\|Length\|ToolCalls\|ContentFilter\|Cancelled, usage: Usage { prompt_tokens, completion_tokens } }` |
| Provider trait | `ModelProvider: Send + Sync { descriptor() -> &ProviderDescriptor; models() -> Result<Vec<ModelDescriptor>, ProviderError>; chat(&ChatRequest, &mut dyn FnMut(&StreamEvent), &CancelToken) -> Result<ChatResponse, ProviderError>; health() -> ProviderHealth }`; `ProviderDescriptor { id, kind: OpenaiCompatible\|OpenaiResponses\|Anthropic\|Gemini\|AzureOpenai\|Bedrock\|LlamaCpp\|Fake, base_url, locality, auth: AuthConfig::None\|VaultSecret{secret_name} }`; `ProviderHealth { status: Ready\|AuthRequired\|Unavailable\|Refused, error, message, latency_ms, checked_at }`                                                                                                                                           |
| Cancellation   | `CancelToken` (re-exported from `aidex-core`): shared `Arc<AtomicBool>` plus optional deadline; `cancel()`, `with_timeout()`, `state() -> Option<Cancelled\|DeadlineExceeded>`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Errors         | `ProviderError::{Unavailable{reason}, Unauthorized, Credential(CredentialError), RateLimited{retry_after}, ContextOverflow{limit, requested}, OfflineRefused{host}, PolicyDenied{reason}, Cancelled, Timeout, Protocol(String), Http{status, code}, InvalidRequest(String), ModelNotFound{model}}` with `kind()` (stable snake_case `ProviderErrorKind`), `code()` (`ServiceErrorCode`), and `is_retryable()` (unavailable, rate limited, timeout, HTTP 408/425/429/5xx only)                                                                                                                                                                                                                                                   |
| Credentials    | `CredentialResolver::resolve(&ProviderId, secret_name) -> Result<SecretBytes, CredentialError>`; `SecretBytes` (zeroizing, `Debug` = `[REDACTED]`, not `Clone`/`Serialize`/`Display`); `NoVault` (always unavailable); `StaticCredentials` (in-memory, per provider; tests and embedders holding a per-launch key)                                                                                                                                                                                                                                                                                                                                                                                                              |
| Egress         | `EgressGate::check_remote(&ProviderId, host)`; `OfflineMode` (`new`, `load(&DrivePaths)` persisted in `private_config/network.toml`, `is_offline`, `set_offline`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Providers      | `FakeProvider` (+ `FakeScript`, `FakeTurn::{text, tool_call, tool_calls, error}.with_delay().chunked()`, `requests()`, `set_models_error`, `set_health_error`); `OpenAiCompatibleProvider::{from_config, with_options}`; the bare `OpenAiClient { list_models, chat }` over a `BaseUrl` with `HttpOptions`                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Registry       | `ProviderRegistry::{load(&DrivePaths, resolver, gate), load_with(.., HttpOptions), in_memory, with_observer, add_provider, update_provider, remove_provider, register(Box<dyn ModelProvider>), register_shared(Arc<..>), unregister, providers, provider, provider_config, models() -> ModelListing { models, failures }, default_model, set_default_model, clear_default_model, health}`                                                                                                                                                                                                                                                                                                                                       |
| Events         | `ModelObserver::observe(&ModelEvent)`; `ModelEvent::{Selected, Fallback(FallbackEvent { from, to, reason, message }), DefaultChanged, ProviderAdded, ProviderRemoved, NetworkModeChanged}` (dotted names `model.selected`, `model.fallback`, ...); `NoopObserver`, `RecordingObserver`                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Configuration  | `ProvidersConfig { version: 1, default_model, provider: [ProviderConfig { id, kind, base_url, locality?, auth, discover_models, model: [ModelConfig] }] }` in `private_config/providers.toml`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |

## `providers.toml`

```toml
version = 1
default_model = "openai/gpt-4o-mini"

[[provider]]
id = "openai"
kind = "openai-compatible"
base_url = "https://api.openai.com/v1"
discover_models = true

[provider.auth]
type = "vault_secret"
secret_name = "openai-key"   # stored with `aidex vault set openai-key --consumer provider:openai`

[[provider.model]]
id = "gpt-4o-mini"
context_window = 128000
max_output_tokens = 16384
tool_calls = "native"
json_schema_output = true
roles = ["code", "general"]
```

The schema is closed (`deny_unknown_fields`, version checked): a pasted `api_key = ...` makes the file invalid rather than being used, and parse errors report a line number with quoted values removed, so a misplaced key is never echoed. Writes are atomic (temp file, fsync, rename) and refuse symlinked paths.

## Guarantees

- **Credentials** are resolved per request, used only to build protocol-specific authentication headers or AWS signatures on a worker thread, and zeroized when dropped. They never appear in configuration, `Debug` output, errors (HTTP failures carry the status and a sanitized machine code, never server text, which can echo the key), observer events, request bodies, or model context. Tests generate the credential at run time and scan config files, `Debug`, errors, events, listings, and the bytes sent to a fake server.
- **Offline / egress**: remote providers consult the `EgressGate` before resolving a credential and before any socket is opened; offline mode returns `OfflineRefused` with zero connections (tested against a listener that counts connections). Even cached listings are gated, so an offline listing reports the refusal.
- **Locality is derived from the endpoint**: only loopback hosts (`localhost`, `127.0.0.0/8`, `::1`) can be local; LAN and cloud endpoints are remote and cannot be declared local. A loopback endpoint can be forced `remote` (for example a tunnel). Credentials are sent only over `https` or to loopback; URLs with embedded user info, queries, or fragments are rejected.
- **HTTP**: redirects are never followed (a redirect could carry the `Authorization` header elsewhere); environment proxies are ignored for loopback endpoints. Bodies, SSE lines and aggregate data events (1 MiB each), streamed text (8 MiB), tool calls (64), and arguments (1 MiB each) are bounded.
- **Failures are explicit**: unreachable servers are `Unavailable`, a stream that ends without `[DONE]`/finish reason is a protocol error (never a truncated "success"), discovery failures are errors (the registry reports them per provider next to working providers), and a model without reported limits is marked `Assumed` (the router will not auto-select it).
- **Cancellation**: `chat` runs the exchange on a worker thread and returns `Cancelled` (after `Done(Cancelled)`) or `Timeout` within one 20 ms poll of the token firing, even while the server is silent. Tokens already stopped win before credential resolution and cause no connection; transport admission rechecks the token before starting the worker and before its send. The worker closes the connection at its next read, which stops generation on servers such as llama-server.
- `FakeProvider` is deterministic, performs no I/O, records requests, enforces its models' context windows, and fails an exhausted script with a non-retryable protocol error so fallback cannot hide a test bug.

## OpenAI-compatible wire behaviour

`base_url` is the API root including its version segment; an empty path becomes `/v1`. Chat always requests streaming (`stream: true`, `stream_options.include_usage`) and also accepts a plain JSON completion. Tools map to `tools`/`tool_choice` (`function` type), JSON-schema output to `response_format: {type: "json_schema", json_schema: {name, schema, strict}}`, which llama-server enforces with grammar-constrained decoding. Discovery reads `/models` and the context fields `context_length`, `max_model_len`, `context_window`, or llama.cpp `meta.n_ctx`/`meta.n_ctx_train` (the training context can exceed the server's configured context; such overflows surface as `ContextOverflow`). Error bodies are parsed only for `code`/`type`, `n_ctx`, and `n_prompt_tokens` (llama-server's `exceed_context_size_error`, OpenAI's `context_length_exceeded`).

## Limitations / not implemented

- API-key presets expose `oauth_supported: false`. ChatGPT OAuth acquisition belongs to `aidex-auth`, which registers Responses instances and resolves their vault credentials. Generic device login is not implemented. Missing/rejected credentials report `auth_required` health.
- Azure and Bedrock require configured deployment/model IDs (`discover_models: false`). Bedrock has no free runtime liveness endpoint; `health()` explicitly reports an unavailable check, without making a billable inference request.
- `max_tokens` is sent as `max_tokens`; OpenAI reasoning models that require `max_completion_tokens` are not special-cased.
- `reasoning_content` deltas and multiple choices (`n > 1`) are ignored; vision content parts and embeddings requests are not part of the chat API yet.
- The header value is copied into `ureq`'s request buffers, which are not zeroized.
- Other health checks use `GET /models`; Azure adds its API version. An endpoint that does not offer a listing returns the real error.
- `OfflineMode` is the only egress gate; the broker's policy-backed gate (DNS re-checks, allowlists) replaces it in #17.

## Cloud protocols and presets

`ProviderRegistry::add_provider` and `update_provider` dispatch on `ProviderKind`;
`OpenAiCompatibleProvider` accepts only `openai-compatible`. Accounts use an
instance `ProviderId`, independently of the stable preset ID returned by
`provider_presets()`. Presets describe adapter support; configured/discovered
model metadata remains authoritative for routing.

| Kind                | Base URL                                           | Wire/authentication                                                             |
| ------------------- | -------------------------------------------------- | ------------------------------------------------------------------------------- |
| `openai-compatible` | versioned API root                                 | `/chat/completions`, Bearer vault key                                           |
| `openai-responses`  | `https://api.openai.com/v1`                        | `/responses`, Bearer vault key                                                  |
| `anthropic`         | `https://api.anthropic.com/v1`                     | `/messages`, `x-api-key`, `anthropic-version: 2023-06-01`                       |
| `gemini`            | `https://generativelanguage.googleapis.com/v1beta` | `/models/<id>:streamGenerateContent?alt=sse`, `x-goog-api-key`                  |
| `azure-openai`      | `https://<resource>.openai.azure.com/openai`       | `/deployments/<id>/chat/completions?api-version=2024-10-21`, `api-key`          |
| `bedrock`           | `https://bedrock-runtime.<region>.amazonaws.com`   | `/model/<id>/converse-stream`, AWS SigV4, CRC-validated binary AWS event stream |

Bedrock's vault secret contains a JSON object with `access_key_id`,
`secret_access_key`, `region`, and optional `session_token`; all credentials
are released only for `provider:<instance-id>` and zeroized after signing.
AWS credential discovery from the environment is deliberately absent.

The catalog also includes OpenRouter, xAI, DeepSeek, Groq, Mistral and custom
compatible endpoints. No model ID list is hard-coded; add custom IDs through
`ModelConfig` or use discovery. Configured limits/capabilities override discoveries;
unknown limits remain `Assumed` so automatic routing cannot treat them as verified.

Requests map text, assistant function calls, tool results, tool schemas/choices,
token limits, temperature and supported structured output into each protocol.
Responses and Anthropic assemble split function argument events; Gemini maps
complete `functionCall` objects; Bedrock assembles tool-use input fragments.
Noncompatible endpoints must actually return their streaming protocol. Malformed,
CRC-corrupt, incomplete and invalid tool JSON responses fail explicitly.

OpenAI Responses also accepts the ChatGPT plan-token model catalog's
`models`/`slug` envelope, displaying only entries with `visibility: "list"`
and retaining their supplied labels and ordering at the provider boundary.
API-key `data`/`id` catalogs remain supported. Responses inference uses
`store: false` and `stream: true`. Recognized ChatGPT plan eligibility and
usage failures retain sanitized status/code information and stop automatic
provider or billing fallback; callers may explicitly retry temporary
availability later.

Anthropic/Bedrock JSON-schema output is explicitly refused by this adapter.
Gemini function-call signatures are retained in optional `ToolCall.provider_metadata` and the original signed part is replayed during tool continuations. Optional non-function reasoning signatures and multimodal parts remain outside the current neutral chat contract. OpenAI Responses refuses stop sequences.
Azure API version is currently fixed to the documented 2024-10-21 deployment API.
All fixtures use loopback servers; production vendor calls require user credentials
and have not been exercised by these tests.

Official protocol references: [OpenAI streaming events](https://developers.openai.com/api/reference/resources/responses/streaming-events),
[Anthropic streaming](https://platform.claude.com/docs/en/build-with-claude/streaming),
[Gemini functions](https://ai.google.dev/gemini-api/docs/function-calling),
[Azure REST authentication](https://learn.microsoft.com/en-us/azure/foundry/openai/reference),
[Bedrock ConverseStream](https://docs.aws.amazon.com/bedrock/latest/APIReference/API_runtime_ConverseStream.html),
[AWS signing](https://docs.aws.amazon.com/IAM/latest/UserGuide/reference_sigv-create-signed-request.html).
