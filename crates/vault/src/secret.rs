//! Zeroizing containers for secret material. They are never `Clone` or
//! `Serialize`, and their `Debug`/`Display` output is `[REDACTED]`.

use std::fmt;
use zeroize::Zeroizing;

pub(crate) const REDACTED: &str = "[REDACTED]";

/// A secret value (credential bytes). The buffer is zeroized on drop.
///
/// Values leave the vault only through [`crate::UnlockedVault::resolve`]; callers
/// read them with [`SecretBytes::expose`] for the shortest possible time.
pub struct SecretBytes(Zeroizing<Vec<u8>>);

impl SecretBytes {
    /// Take ownership of `bytes` without copying them.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Borrow the secret bytes. Do not log, persist, or copy them into long-lived buffers.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
        write!(f, "SecretBytes({REDACTED})")
    }
}

impl fmt::Display for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// A secret text value such as a passphrase. The buffer is zeroized on drop.
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    /// Take ownership of `text` without copying it.
    pub fn new(text: String) -> Self {
        Self(Zeroizing::new(text))
    }

    /// Borrow the secret text. Do not log, persist, or copy it into long-lived buffers.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for SecretString {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretString({REDACTED})")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_containers_never_format_their_contents() {
        let bytes = SecretBytes::from("sk-live-123".to_owned());
        let text = SecretString::from("correct horse".to_owned());
        for rendered in [
            format!("{bytes:?}"),
            format!("{bytes}"),
            format!("{text:?}"),
            format!("{text}"),
            format!("{:#?}", (&bytes, &text)),
        ] {
            assert!(rendered.contains(REDACTED), "{rendered}");
            assert!(!rendered.contains("sk-live"), "{rendered}");
            assert!(!rendered.contains("horse"), "{rendered}");
        }
        assert_eq!(bytes.expose(), b"sk-live-123");
        assert_eq!(text.expose(), "correct horse");
        assert_eq!((bytes.len(), text.is_empty()), (11, false));
    }
}
