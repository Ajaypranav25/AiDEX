//! Network modes (spec §12.5), access settings, and domain allowlist patterns.

use std::{fmt, net::IpAddr, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{PolicyError, error::truncate, macros::string_enum};

string_enum! {
    /// Session network mode, from most to least restrictive.
    pub enum NetworkMode("network mode") {
        /// No egress beyond loopback (and the local network only if allowed).
        Offline => "offline",
        /// Only allowlisted domains.
        Allowlist => "allowlist",
        /// Any public domain; IP literals and local networks per settings.
        Normal => "normal",
        /// No network restriction from the mode itself.
        Unrestricted => "unrestricted",
    }
}

string_enum! {
    /// How the network mode treats a class of destinations.
    pub enum NetworkAccess("network access") {
        Allow => "allow",
        Ask => "ask",
        Deny => "deny",
    }
}

/// Global network settings loaded from policy documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkSettings {
    /// Domains permitted in `allowlist` mode.
    pub allow_domains: Vec<DomainPattern>,
    /// Private/link-local/LAN destinations (default `ask`).
    pub local_network: NetworkAccess,
    /// Literal public IP addresses (default `ask`).
    pub arbitrary_ip: NetworkAccess,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            allow_domains: Vec::new(),
            local_network: NetworkAccess::Ask,
            arbitrary_ip: NetworkAccess::Ask,
        }
    }
}

impl NetworkSettings {
    pub fn allows_domain(&self, host: &str) -> bool {
        self.allow_domains
            .iter()
            .any(|pattern| pattern.matches(host))
    }
}

/// `example.com` (exactly that name) or `*.example.com` (any subdomain, not the apex).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DomainPattern {
    name: String,
    subdomains: bool,
}

impl DomainPattern {
    pub fn parse(pattern: &str) -> Result<Self, PolicyError> {
        let invalid = || PolicyError::unknown("domain pattern", pattern);
        let lower = pattern.trim().to_ascii_lowercase();
        let (subdomains, name) = match lower.strip_prefix("*.") {
            Some(rest) => (true, rest),
            None => (false, lower.as_str()),
        };
        let name = name.strip_suffix('.').unwrap_or(name);
        let valid = !name.is_empty()
            && name.len() <= 253
            && name.parse::<IpAddr>().is_err()
            && name.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            });
        if !valid {
            return Err(invalid());
        }
        Ok(Self {
            name: name.to_owned(),
            subdomains,
        })
    }

    /// Whether `host` (already normalized by classification) matches.
    pub fn matches(&self, host: &str) -> bool {
        let host = host.strip_suffix('.').unwrap_or(host);
        if self.subdomains {
            host.len() > self.name.len()
                && host
                    .strip_suffix(self.name.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
        } else {
            host.eq_ignore_ascii_case(&self.name)
        }
    }
}

impl fmt::Display for DomainPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.subdomains {
            write!(f, "*.{}", self.name)
        } else {
            f.write_str(&self.name)
        }
    }
}

impl FromStr for DomainPattern {
    type Err = PolicyError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for DomainPattern {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DomainPattern {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(|_| {
            de::Error::custom(format!("invalid domain pattern `{}`", truncate(&value)))
        })
    }
}
