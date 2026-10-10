//! Credential resolution at request time (spec §40, §92).
//!
//! Provider configuration names a vault secret; the bytes are fetched through a
//! [`CredentialResolver`] for each request, used only to build the HTTP
//! `Authorization` header, and zeroized when dropped. They never enter
//! configuration files, events, errors, `Debug` output, or model context.

use std::{collections::HashMap, fmt, sync::Mutex};

use zeroize::Zeroizing;

use crate::{CredentialError, ProviderId};

/// Secret bytes that are zeroized on drop and never printed or serialized.
///
/// Deliberately not `Clone`, `Serialize`, or `Display`; `Debug` prints
/// `SecretBytes([REDACTED])`.
pub struct SecretBytes(Zeroizing<Vec<u8>>);

impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Borrow the secret for immediate use (building a header).
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `Authorization: Bearer <token>` value, zeroized on drop. Tokens must be
    /// non-empty printable ASCII without spaces.
    pub(crate) fn bearer_header(&self) -> Option<Zeroizing<String>> {
        let token = std::str::from_utf8(self.expose()).ok()?;
        let token = token.trim();
        if token.is_empty() || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return None;
        }
        let mut header = Zeroizing::new(String::with_capacity(7 + token.len()));
        header.push_str("Bearer ");
        header.push_str(token);
        Some(header)
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl From<String> for SecretBytes {
    fn from(text: String) -> Self {
        Self::new(text.into_bytes())
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
    }
}

/// Produces a provider's credential for one request.
///
/// The daemon implements this over vault handles scoped to the consumer
/// `provider:<id>`. Implementations must not read secrets from environment
/// variables or configuration files, and must not log what they return.
pub trait CredentialResolver: Send + Sync {
    fn resolve(
        &self,
        provider: &ProviderId,
        secret_name: &str,
    ) -> Result<SecretBytes, CredentialError>;
}

/// A resolver for builds without a vault: every lookup fails with
/// [`CredentialError::Unavailable`], so providers that need a credential refuse
/// to send anything while unauthenticated providers keep working.
#[derive(Clone, Debug)]
pub struct NoVault {
    reason: String,
}

impl NoVault {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl Default for NoVault {
    fn default() -> Self {
        Self::new("vault unavailable")
    }
}

impl CredentialResolver for NoVault {
    fn resolve(&self, _: &ProviderId, _: &str) -> Result<SecretBytes, CredentialError> {
        Err(CredentialError::Unavailable {
            reason: self.reason.clone(),
        })
    }
}

/// An in-memory resolver for tests and embedders that already hold a
/// credential (for example a per-launch key for a managed local server).
/// Each secret is bound to one provider.
#[derive(Default)]
pub struct StaticCredentials {
    secrets: Mutex<HashMap<SecretKey, Zeroizing<Vec<u8>>>>,
}

/// A secret is addressed by the provider it is bound to and its name.
type SecretKey = (ProviderId, String);

impl StaticCredentials {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &self,
        provider: ProviderId,
        secret_name: impl Into<String>,
        secret: SecretBytes,
    ) {
        if let Ok(mut secrets) = self.secrets.lock() {
            secrets.insert(
                (provider, secret_name.into()),
                Zeroizing::new(secret.expose().to_vec()),
            );
        }
    }

    pub fn remove(&self, provider: &ProviderId, secret_name: &str) {
        if let Ok(mut secrets) = self.secrets.lock() {
            secrets.remove(&(provider.clone(), secret_name.to_owned()));
        }
    }
}

impl fmt::Debug for StaticCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.secrets.lock().map_or(0, |secrets| secrets.len());
        f.debug_struct("StaticCredentials")
            .field("secrets", &format_args!("[{count} REDACTED]"))
            .finish()
    }
}

impl CredentialResolver for StaticCredentials {
    fn resolve(
        &self,
        provider: &ProviderId,
        secret_name: &str,
    ) -> Result<SecretBytes, CredentialError> {
        let secrets = self
            .secrets
            .lock()
            .map_err(|_| CredentialError::Unavailable {
                reason: "credential store is poisoned".into(),
            })?;
        secrets
            .get(&(provider.clone(), secret_name.to_owned()))
            .map(|secret| SecretBytes::new(secret.to_vec()))
            .ok_or_else(|| CredentialError::NotFound {
                secret_name: secret_name.to_owned(),
            })
    }
}

/// Whether `name` is an acceptable vault secret name (the `aidex-vault` rule):
/// 1-128 ASCII letters, digits, `.`, `_`, or `-`, starting with a letter or digit.
pub fn is_secret_name(name: &str) -> bool {
    name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_redacted_and_bearer_tokens_are_checked() {
        let secret = SecretBytes::from("sk-very-secret".to_owned());
        assert_eq!(format!("{secret:?}"), "SecretBytes([REDACTED])");
        assert_eq!(
            secret.bearer_header().unwrap().as_str(),
            "Bearer sk-very-secret"
        );
        for bad in ["", "two words", "line\nbreak", "ü"] {
            assert!(
                SecretBytes::from(bad.to_owned()).bearer_header().is_none(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn static_credentials_bind_secrets_to_one_provider() {
        let store = StaticCredentials::new();
        let openai = ProviderId::new("openai").unwrap();
        let other = ProviderId::new("other").unwrap();
        store.insert(
            openai.clone(),
            "key",
            SecretBytes::from("sk-abc".to_owned()),
        );
        assert_eq!(store.resolve(&openai, "key").unwrap().expose(), b"sk-abc");
        assert!(matches!(
            store.resolve(&other, "key"),
            Err(CredentialError::NotFound { .. })
        ));
        assert!(!format!("{store:?}").contains("sk-abc"));
        assert!(matches!(
            NoVault::default().resolve(&openai, "key"),
            Err(CredentialError::Unavailable { .. })
        ));
    }
}
