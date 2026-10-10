//! Classification of network destinations into loopback, local network, public IP, or domain.

use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{PolicyError, error::truncate, macros::string_enum};

/// Longest DNS name accepted (RFC 1035).
const MAX_HOST_LEN: usize = 253;

string_enum! {
    /// Where a network destination is, as far as the policy can tell before DNS resolution.
    pub enum NetworkKind("network kind") {
        /// `localhost`, `*.localhost`, `127.0.0.0/8`, `::1`.
        Loopback => "loopback",
        /// Private, link-local, CGNAT, multicast, unspecified, single-label, or local-suffix names.
        LocalNetwork => "local_network",
        /// A literal public IP address.
        PublicIp => "public_ip",
        /// A public DNS name.
        Domain => "domain",
    }
}

/// A classified network destination. The kind is always computed from the host by
/// [`classify_network`]; deserialization re-classifies and never trusts a supplied kind.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize)]
pub struct NetworkTarget {
    host: String,
    port: u16,
    kind: NetworkKind,
}

impl NetworkTarget {
    /// Normalized host: lowercase, no brackets, no trailing dot.
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn kind(&self) -> NetworkKind {
        self.kind
    }
}

impl fmt::Display for NetworkTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

impl<'de> Deserialize<'de> for NetworkTarget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            host: String,
            port: u16,
            #[serde(default, rename = "kind")]
            _kind: Option<de::IgnoredAny>,
        }
        let wire = Wire::deserialize(deserializer)?;
        classify_network(&wire.host, wire.port).map_err(de::Error::custom)
    }
}

/// Classify a destination host and port.
///
/// Hosts are IP literals (IPv6 may be bracketed) or DNS names. Non-canonical numeric
/// hosts such as `2130706433` or `0x7f.1`, which some clients silently treat as IP
/// addresses, are rejected rather than guessed. Classification happens before DNS
/// resolution: the broker must still check resolved addresses (DNS rebinding).
pub fn classify_network(host: &str, port: u16) -> Result<NetworkTarget, PolicyError> {
    let invalid = || PolicyError::InvalidNetworkTarget(truncate(host));
    let trimmed = host.trim();
    let unbracketed = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    if let Ok(ip) = unbracketed.parse::<IpAddr>() {
        let kind = classify_ip(ip);
        return Ok(NetworkTarget {
            host: ip.to_string(),
            port,
            kind,
        });
    }
    let name = unbracketed
        .strip_suffix('.')
        .unwrap_or(unbracketed)
        .to_ascii_lowercase();
    if name.is_empty() || name.len() > MAX_HOST_LEN || !valid_dns_name(&name) {
        return Err(invalid());
    }
    let last_label = name.rsplit('.').next().unwrap_or_default();
    if last_label.bytes().all(|byte| byte.is_ascii_digit()) || looks_numeric(&name) {
        return Err(invalid());
    }
    let kind = classify_name(&name);
    Ok(NetworkTarget {
        host: name,
        port,
        kind,
    })
}

fn valid_dns_name(name: &str) -> bool {
    name.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    })
}

/// Every label is decimal, octal, or `0x` hex: an IP address in a non-canonical spelling.
fn looks_numeric(name: &str) -> bool {
    name.split('.').all(|label| {
        let hex = label
            .strip_prefix("0x")
            .is_some_and(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()));
        hex || label.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn classify_name(name: &str) -> NetworkKind {
    if name == "localhost" || name.ends_with(".localhost") {
        return NetworkKind::Loopback;
    }
    const LOCAL_SUFFIXES: [&str; 5] = [".local", ".lan", ".home.arpa", ".internal", ".localdomain"];
    if !name.contains('.') || LOCAL_SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
        return NetworkKind::LocalNetwork;
    }
    NetworkKind::Domain
}

fn classify_ip(ip: IpAddr) -> NetworkKind {
    match ip {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => classify_v4(v4),
            None => classify_v6(v6),
        },
    }
}

fn classify_v4(ip: Ipv4Addr) -> NetworkKind {
    let [a, b, ..] = ip.octets();
    let cgnat = a == 100 && (64..128).contains(&b);
    if ip.is_loopback() {
        NetworkKind::Loopback
    } else if ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || a == 0
        || cgnat
    {
        NetworkKind::LocalNetwork
    } else {
        NetworkKind::PublicIp
    }
}

fn classify_v6(ip: Ipv6Addr) -> NetworkKind {
    let first = ip.segments()[0];
    let unique_local = first & 0xfe00 == 0xfc00;
    let link_local = first & 0xffc0 == 0xfe80;
    if ip.is_loopback() {
        NetworkKind::Loopback
    } else if ip.is_unspecified() || ip.is_multicast() || unique_local || link_local {
        NetworkKind::LocalNetwork
    } else {
        NetworkKind::PublicIp
    }
}
