//! Stable vendor identities describe supported protocols, not configured accounts.
use crate::ProviderKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderPreset {
    pub id: String,
    pub display_name: String,
    pub kind: ProviderKind,
    pub base_url: Option<String>,
    pub discover_models: bool,
    pub tool_calls: bool,
    pub json_schema_output: bool,
    pub authentication: String,
    /// OAuth account acquisition is deliberately not implemented here.
    pub oauth_supported: bool,
}

pub fn provider_presets() -> Vec<ProviderPreset> {
    [
        (
            "openai",
            "OpenAI Responses",
            ProviderKind::OpenaiResponses,
            Some("https://api.openai.com/v1"),
            true,
            true,
        ),
        (
            "openai-chat",
            "OpenAI Chat Completions",
            ProviderKind::OpenaiCompatible,
            Some("https://api.openai.com/v1"),
            true,
            true,
        ),
        (
            "anthropic",
            "Anthropic",
            ProviderKind::Anthropic,
            Some("https://api.anthropic.com/v1"),
            true,
            false,
        ),
        (
            "gemini",
            "Google Gemini",
            ProviderKind::Gemini,
            Some("https://generativelanguage.googleapis.com/v1beta"),
            true,
            true,
        ),
        (
            "azure-openai",
            "Azure OpenAI",
            ProviderKind::AzureOpenai,
            None,
            false,
            true,
        ),
        (
            "bedrock",
            "Amazon Bedrock",
            ProviderKind::Bedrock,
            None,
            false,
            false,
        ),
        (
            "openrouter",
            "OpenRouter",
            ProviderKind::OpenaiCompatible,
            Some("https://openrouter.ai/api/v1"),
            true,
            true,
        ),
        (
            "xai",
            "xAI",
            ProviderKind::OpenaiCompatible,
            Some("https://api.x.ai/v1"),
            true,
            true,
        ),
        (
            "deepseek",
            "DeepSeek",
            ProviderKind::OpenaiCompatible,
            Some("https://api.deepseek.com/v1"),
            true,
            false,
        ),
        (
            "groq",
            "Groq",
            ProviderKind::OpenaiCompatible,
            Some("https://api.groq.com/openai/v1"),
            true,
            true,
        ),
        (
            "mistral",
            "Mistral",
            ProviderKind::OpenaiCompatible,
            Some("https://api.mistral.ai/v1"),
            true,
            true,
        ),
        (
            "custom",
            "Custom compatible endpoint",
            ProviderKind::OpenaiCompatible,
            None,
            true,
            true,
        ),
    ]
    .into_iter()
    .map(
        |(id, display_name, kind, url, discover_models, json_schema_output)| ProviderPreset {
            id: id.into(),
            display_name: display_name.into(),
            kind,
            base_url: url.map(str::to_owned),
            discover_models,
            tool_calls: true,
            json_schema_output,
            authentication: if kind == ProviderKind::Bedrock {
                "aws_sigv4_vault"
            } else {
                "api_key_vault"
            }
            .into(),
            oauth_supported: false,
        },
    )
    .collect()
}
