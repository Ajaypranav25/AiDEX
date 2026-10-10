//! Endpoint parsing restricted to what a provider base URL may contain.

use std::{fmt, net::IpAddr};

const MAX_URL: usize = 2048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scheme {
    Http,
    Https,
}

/// A validated provider API root such as `https://api.openai.com/v1`.
///
/// Only `http` and `https` with a host, optional port, and path are accepted:
/// user info (credentials embedded in the URL), queries, fragments, whitespace,
/// and control characters are rejected. An empty path becomes `/v1`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaseUrl {
    scheme: Scheme,
    /// Lowercase host name or IP literal (IPv6 without brackets).
    host: String,
    port: u16,
    explicit_port: bool,
    /// Path without a trailing `/`, beginning with `/`.
    path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidUrl(&'static str);

impl fmt::Display for InvalidUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid base URL: {}", self.0)
    }
}

impl std::error::Error for InvalidUrl {}

impl BaseUrl {
    /// An API whose root has no implicit version segment (AWS Bedrock).
    pub(crate) fn parse_root(text: &str) -> Result<Self, InvalidUrl> {
        let mut base = Self::parse(text)?;
        let rest = text
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or_default();
        if !rest.contains('/') || rest.ends_with('/') && rest.matches('/').count() == 1 {
            base.path.clear();
        }
        Ok(base)
    }
    pub fn parse(text: &str) -> Result<Self, InvalidUrl> {
        if text.len() > MAX_URL {
            return Err(InvalidUrl("longer than 2048 bytes"));
        }
        if text.chars().any(|ch| ch.is_whitespace() || ch.is_control()) || !text.is_ascii() {
            return Err(InvalidUrl(
                "must be ASCII without spaces or control characters",
            ));
        }
        let (scheme, rest) = if let Some(rest) = strip_prefix_ignore_case(text, "https://") {
            (Scheme::Https, rest)
        } else if let Some(rest) = strip_prefix_ignore_case(text, "http://") {
            (Scheme::Http, rest)
        } else {
            return Err(InvalidUrl("scheme must be http:// or https://"));
        };
        if rest.contains(['?', '#']) {
            return Err(InvalidUrl("queries and fragments are not allowed"));
        }
        let (authority, path) = rest.find('/').map_or((rest, ""), |at| rest.split_at(at));
        if authority.contains('@') {
            return Err(InvalidUrl(
                "credentials must not be embedded in the URL; store them in the vault",
            ));
        }
        let (host, port) = split_host_port(authority)?;
        let default_port = match scheme {
            Scheme::Http => 80,
            Scheme::Https => 443,
        };
        let path = path.trim_end_matches('/');
        if path
            .split('/')
            .any(|segment| segment == ".." || segment == ".")
        {
            return Err(InvalidUrl("path must not contain `.` or `..` segments"));
        }
        if path.contains('%') {
            return Err(InvalidUrl("percent-encoded paths are not supported"));
        }
        Ok(Self {
            scheme,
            host,
            port: port.unwrap_or(default_port),
            explicit_port: port.is_some(),
            path: if path.is_empty() {
                "/v1".into()
            } else {
                path.into()
            },
        })
    }

    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Whether the host is this machine's loopback interface (`localhost`,
    /// `127.0.0.0/8`, `::1`). Only loopback endpoints count as local.
    pub fn is_loopback(&self) -> bool {
        if self.host == "localhost" {
            return true;
        }
        self.host
            .parse::<IpAddr>()
            .is_ok_and(|address| match address {
                IpAddr::V4(v4) => v4.is_loopback(),
                IpAddr::V6(v6) => {
                    v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
                }
            })
    }

    /// The URL of `endpoint` (for example `chat/completions`) under the root.
    pub fn join(&self, endpoint: &str) -> String {
        format!("{}/{}", self, endpoint.trim_start_matches('/'))
    }
}

impl fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scheme = match self.scheme {
            Scheme::Http => "http",
            Scheme::Https => "https",
        };
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.explicit_port {
            write!(f, "{scheme}://{host}:{}{}", self.port, self.path)
        } else {
            write!(f, "{scheme}://{host}{}", self.path)
        }
    }
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    (text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| &text[prefix.len()..])
}

fn split_host_port(authority: &str) -> Result<(String, Option<u16>), InvalidUrl> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest
            .split_once(']')
            .ok_or(InvalidUrl("unterminated IPv6 literal"))?;
        host.parse::<std::net::Ipv6Addr>()
            .map_err(|_| InvalidUrl("invalid IPv6 literal"))?;
        let port = match after {
            "" => None,
            _ => Some(
                after
                    .strip_prefix(':')
                    .ok_or(InvalidUrl("unexpected text after IPv6 literal"))?,
            ),
        };
        (host.to_ascii_lowercase(), port)
    } else {
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        let host = host.to_ascii_lowercase();
        let valid = !host.is_empty()
            && host.len() <= 253
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            && !host.starts_with(['-', '.'])
            && !host.contains("..");
        if !valid {
            return Err(InvalidUrl("host must be a DNS name or IP address"));
        }
        (host, port)
    };
    let port = match port {
        None => None,
        Some(text) => Some(
            text.parse::<u16>()
                .ok()
                .filter(|port| *port != 0 && !text.starts_with('+'))
                .ok_or(InvalidUrl("port must be 1-65535"))?,
        ),
    };
    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_api_roots_and_classifies_loopback() {
        let openai = BaseUrl::parse("https://api.openai.com/v1/").unwrap();
        assert_eq!(openai.to_string(), "https://api.openai.com/v1");
        assert_eq!(
            openai.join("chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert!(!openai.is_loopback());
        let local = BaseUrl::parse("http://127.0.0.1:8080").unwrap();
        assert_eq!(local.to_string(), "http://127.0.0.1:8080/v1");
        assert!(local.is_loopback());
        assert!(
            BaseUrl::parse("http://LOCALHOST:1/v1")
                .unwrap()
                .is_loopback()
        );
        assert!(BaseUrl::parse("http://[::1]:9/v1").unwrap().is_loopback());
        assert_eq!(
            BaseUrl::parse("http://[::1]:9/v1").unwrap().to_string(),
            "http://[::1]:9/v1"
        );
        for lan in [
            "http://192.168.1.5:8000/v1",
            "http://10.0.0.1/v1",
            "http://localhost.example.com",
        ] {
            assert!(!BaseUrl::parse(lan).unwrap().is_loopback(), "{lan}");
        }
    }

    #[test]
    fn rejects_embedded_credentials_and_unsupported_forms() {
        for bad in [
            "https://user:sk-secret@api.example.com/v1",
            "ftp://example.com",
            "api.openai.com/v1",
            "https://example.com/v1?key=1",
            "https://example.com/v1#x",
            "https://exa mple.com",
            "https://example.com:0",
            "https://example.com:99999",
            "https://[::1",
            "https:///v1",
            "https://example.com/../v1",
            "https://example.com/%2e",
        ] {
            assert!(BaseUrl::parse(bad).is_err(), "{bad}");
        }
        let error = BaseUrl::parse("https://u:sk-secret@h/v1")
            .unwrap_err()
            .to_string();
        assert!(!error.contains("sk-secret"));
    }
}
