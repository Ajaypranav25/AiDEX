//! What a caller asks the router for.

use std::{fmt, str::FromStr};

use aidex_providers::{ModelRef, ModelRole};
use serde::{Deserialize, Serialize};

macro_rules! named_enum {
    ($(#[$meta:meta])* pub enum $name:ident ($what:literal) { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($(#[$vmeta])* $variant),+ }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = String;
            /// Accepts `snake_case` or `kebab-case`.
            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let normalized = text.replace('-', "_");
                Self::ALL
                    .iter()
                    .copied()
                    .find(|value| value.as_str() == normalized)
                    .ok_or_else(|| {
                        let names: Vec<&str> = Self::ALL.iter().map(|value| value.as_str()).collect();
                        format!("unknown {} (use {})", $what, names.join(", "))
                    })
            }
        }
    };
}

named_enum! {
    /// The kind of work a model is routed for.
    pub enum TaskKind("task") {
        Plan => "plan",
        Code => "code",
        Edit => "edit",
        Summarize => "summarize",
        Classify => "classify",
        Embed => "embed",
        Verify => "verify",
        Chat => "chat",
    }
}

impl TaskKind {
    /// The model role preferred for this task.
    pub const fn preferred_role(self) -> ModelRole {
        match self {
            Self::Plan => ModelRole::Plan,
            Self::Code | Self::Edit => ModelRole::Code,
            Self::Summarize => ModelRole::Summarize,
            Self::Classify => ModelRole::Classify,
            Self::Embed => ModelRole::Embed,
            Self::Verify => ModelRole::Verify,
            Self::Chat => ModelRole::General,
        }
    }
}

named_enum! {
    /// Privacy modes (spec §82), named as in `aidex-policy`.
    #[derive(Default)]
    pub enum PrivacyMode("privacy mode") {
        /// No external model or service.
        LocalOnly => "local_only",
        /// Local by default, remote models when needed.
        #[default]
        Hybrid => "hybrid",
        /// Remote models first, local fallback.
        CloudPreferred => "cloud_preferred",
        /// Networking disabled.
        AirGapped => "air_gapped",
    }
}

named_enum! {
    #[derive(Default)]
    pub enum NetworkState("network state") {
        #[default]
        Online => "online",
        Offline => "offline",
    }
}

/// Host facts relevant to local model selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostSummary {
    /// Physical RAM, when known.
    pub ram_bytes: Option<u64>,
    pub gpu: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRequest {
    pub task: TaskKind,
    #[serde(default)]
    pub privacy: PrivacyMode,
    #[serde(default)]
    pub network: NetworkState,
    /// Tokens the prompt plus expected output need.
    #[serde(default)]
    pub required_context: u32,
    #[serde(default)]
    pub needs_tools: bool,
    #[serde(default)]
    pub needs_json_schema: bool,
    /// The task handles secrets; remote models are excluded.
    #[serde(default)]
    pub secret_heavy: bool,
    /// An explicit model choice: used if it satisfies every hard constraint,
    /// otherwise routing fails (it is never silently swapped).
    #[serde(default)]
    pub user_override: Option<ModelRef>,
    /// A soft preference (such as the configured default model), ranked first
    /// among eligible candidates and ignored with a reason when ineligible.
    #[serde(default)]
    pub preferred: Option<ModelRef>,
    #[serde(default)]
    pub host: HostSummary,
}

impl RouteRequest {
    pub fn new(task: TaskKind) -> Self {
        Self {
            task,
            privacy: PrivacyMode::default(),
            network: NetworkState::default(),
            required_context: 0,
            needs_tools: false,
            needs_json_schema: false,
            secret_heavy: false,
            user_override: None,
            preferred: None,
            host: HostSummary::default(),
        }
    }

    #[must_use]
    pub fn with_privacy(mut self, privacy: PrivacyMode) -> Self {
        self.privacy = privacy;
        self
    }

    #[must_use]
    pub fn with_network(mut self, network: NetworkState) -> Self {
        self.network = network;
        self
    }

    #[must_use]
    pub fn with_required_context(mut self, tokens: u32) -> Self {
        self.required_context = tokens;
        self
    }

    #[must_use]
    pub fn needing_tools(mut self) -> Self {
        self.needs_tools = true;
        self
    }

    #[must_use]
    pub fn needing_json_schema(mut self) -> Self {
        self.needs_json_schema = true;
        self
    }

    #[must_use]
    pub fn secret_heavy(mut self) -> Self {
        self.secret_heavy = true;
        self
    }

    #[must_use]
    pub fn with_override(mut self, model: ModelRef) -> Self {
        self.user_override = Some(model);
        self
    }

    #[must_use]
    pub fn with_preferred(mut self, model: ModelRef) -> Self {
        self.preferred = Some(model);
        self
    }

    #[must_use]
    pub fn with_host(mut self, host: HostSummary) -> Self {
        self.host = host;
        self
    }

    /// Why remote models are excluded for this request, if they are.
    pub fn remote_block(&self) -> Option<RemoteBlock> {
        if self.privacy == PrivacyMode::AirGapped {
            Some(RemoteBlock::AirGapped)
        } else if self.network == NetworkState::Offline {
            Some(RemoteBlock::Offline)
        } else if self.privacy == PrivacyMode::LocalOnly {
            Some(RemoteBlock::LocalOnly)
        } else if self.secret_heavy {
            Some(RemoteBlock::SecretHeavy)
        } else {
            None
        }
    }
}

named_enum! {
    /// Why a request may only use local models.
    pub enum RemoteBlock("remote block") {
        AirGapped => "air_gapped",
        Offline => "offline",
        LocalOnly => "local_only",
        SecretHeavy => "secret_heavy",
    }
}

impl RemoteBlock {
    pub const fn describe(self) -> &'static str {
        match self {
            Self::AirGapped => "privacy mode air_gapped allows local models only",
            Self::Offline => "network is offline; remote models are excluded",
            Self::LocalOnly => "privacy mode local_only allows local models only",
            Self::SecretHeavy => "secret-heavy tasks stay on local models",
        }
    }
}
