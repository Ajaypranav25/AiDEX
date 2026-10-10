//! Evaluation context: profile, privacy mode (spec §82), host trust (§61), network mode.

use aidex_core::EntityId;

use crate::{NetworkMode, Profile, macros::string_enum};

string_enum! {
    /// Where models and external services may be used.
    pub enum PrivacyMode("privacy mode") {
        /// No external model or service.
        LocalOnly => "local_only",
        /// Local by default, external services when needed.
        Hybrid => "hybrid",
        /// Cloud models primary, local fallback.
        CloudPreferred => "cloud_preferred",
        /// Networking disabled at the broker.
        AirGapped => "air_gapped",
    }
}

impl PrivacyMode {
    /// 0 for no privacy restriction, 1 for local only, 2 for air-gapped.
    pub const fn strictness(self) -> u8 {
        match self {
            Self::Hybrid | Self::CloudPreferred => 0,
            Self::LocalOnly => 1,
            Self::AirGapped => 2,
        }
    }
}

string_enum! {
    /// How much the current host is trusted, most trusted first.
    pub enum HostTrust("host trust") {
        Trusted => "trusted",
        Known => "known",
        New => "new",
        Restricted => "restricted",
        Untrusted => "untrusted",
    }
}

impl HostTrust {
    /// The least strict profile allowed on this host.
    pub const fn minimum_profile(self) -> Profile {
        match self {
            Self::Trusted | Self::Known | Self::New => Profile::Autonomous,
            Self::Restricted => Profile::Balanced,
            Self::Untrusted => Profile::Safe,
        }
    }
}

/// Session-wide inputs to [`crate::evaluate`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationContext {
    pub profile: Profile,
    pub privacy: PrivacyMode,
    pub host_trust: HostTrust,
    pub network_mode: NetworkMode,
    /// Selects machine-layer rules whose subject is this host id.
    pub host_id: Option<EntityId>,
}

impl EvaluationContext {
    pub fn new(
        profile: Profile,
        privacy: PrivacyMode,
        host_trust: HostTrust,
        network_mode: NetworkMode,
    ) -> Self {
        Self {
            profile,
            privacy,
            host_trust,
            network_mode,
            host_id: None,
        }
    }

    pub fn with_host(mut self, host_id: EntityId) -> Self {
        self.host_id = Some(host_id);
        self
    }

    /// The requested profile, tightened to what the host trust level allows.
    pub fn effective_profile(&self) -> Profile {
        Profile::stricter(self.profile, self.host_trust.minimum_profile())
    }

    /// The requested privacy mode, tightened by the profile (Offline Secure ⇒ local only).
    pub fn effective_privacy(&self) -> PrivacyMode {
        let floor = self.effective_profile().minimum_privacy();
        if floor.strictness() > self.privacy.strictness() {
            floor
        } else {
            self.privacy
        }
    }

    /// The requested network mode, capped by the profile and privacy mode.
    pub fn effective_network_mode(&self) -> NetworkMode {
        let mut mode = self
            .network_mode
            .min(self.effective_profile().network_cap());
        if self.effective_privacy() == PrivacyMode::AirGapped {
            mode = NetworkMode::Offline;
        }
        mode
    }
}
