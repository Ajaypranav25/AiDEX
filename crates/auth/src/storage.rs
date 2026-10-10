//! All persisted values, including registration and host metadata, are encrypted.
use crate::{AuthError, oauth::Tokens};
use aidex_vault::{
    ConsumerScope, SecretBytes, SecretKind, SecretName, SecretPolicy, VaultError, VaultErrorKind,
    VaultService,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, time::Duration};
use zeroize::Zeroizing;
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Profile {
    pub client_id: String,
    pub subject: Option<String>,
    pub issuer: Option<String>,
    pub email: Option<String>,
    pub tokens: Option<Tokens>,
    pub needs_reauth: bool,
}
impl Profile {
    pub fn registration(client_id: String) -> Self {
        Self {
            client_id,
            subject: None,
            issuer: None,
            email: None,
            tokens: None,
            needs_reauth: false,
        }
    }
}
fn error(e: VaultError) -> AuthError {
    match e.kind() {
        VaultErrorKind::Locked => AuthError::Locked,
        VaultErrorKind::SecretNotFound => AuthError::NotFound,
        _ => AuthError::Unavailable,
    }
}
fn internal() -> ConsumerScope {
    ConsumerScope::process("aidex-auth").expect("fixed consumer")
}
fn secret(name: &str) -> Result<SecretName, AuthError> {
    SecretName::new(name).map_err(|_| AuthError::Invalid)
}
fn read(
    vault: &VaultService,
    name: &str,
    consumer: ConsumerScope,
) -> Result<SecretBytes, AuthError> {
    let handle = vault
        .issue_handle(&secret(name)?, &consumer, Duration::from_secs(30))
        .map_err(error)?;
    let result = vault.resolve(&handle, &consumer);
    vault.revoke_handle(&handle.handle_id);
    result.map_err(error)
}
fn read_json<T: DeserializeOwned>(
    vault: &VaultService,
    name: &str,
) -> Result<Option<T>, AuthError> {
    match read(vault, name, internal()) {
        Ok(bytes) => serde_json::from_slice(bytes.expose())
            .map(Some)
            .map_err(|_| AuthError::Unavailable),
        Err(AuthError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}
fn write_json(vault: &VaultService, name: &str, value: &impl Serialize) -> Result<(), AuthError> {
    let bytes = Zeroizing::new(serde_json::to_vec(value).map_err(|_| AuthError::Unavailable)?);
    vault
        .put_secret(
            &secret(name)?,
            SecretKind::Generic,
            SecretBytes::new(bytes.to_vec()),
            SecretPolicy::new([internal()]).map_err(error)?,
        )
        .map_err(error)?;
    Ok(())
}
pub(crate) fn validate_provider(provider: &str) -> Result<(), AuthError> {
    ConsumerScope::provider(provider)
        .map(|_| ())
        .map_err(|_| AuthError::Invalid)
}
pub(crate) fn read_profile(
    vault: &VaultService,
    provider: &str,
) -> Result<Option<Profile>, AuthError> {
    read_json(vault, &format!("oauth.{provider}.profile"))
}
pub(crate) fn write_profile(
    vault: &VaultService,
    provider: &str,
    profile: &Profile,
) -> Result<(), AuthError> {
    write_json(vault, &format!("oauth.{provider}.profile"), profile)
}
pub(crate) fn read_access(vault: &VaultService, provider: &str) -> Result<SecretBytes, AuthError> {
    read(
        vault,
        &crate::AuthService::secret_name(provider),
        ConsumerScope::provider(provider).map_err(error)?,
    )
}
pub(crate) fn write_access(
    vault: &VaultService,
    provider: &str,
    access: &[u8],
) -> Result<(), AuthError> {
    vault
        .put_secret(
            &secret(&crate::AuthService::secret_name(provider))?,
            SecretKind::Token,
            SecretBytes::new(access.to_vec()),
            SecretPolicy::new([ConsumerScope::provider(provider).map_err(error)?])
                .map_err(error)?,
        )
        .map_err(error)?;
    Ok(())
}
pub(crate) fn write_access_if_changed(
    vault: &VaultService,
    provider: &str,
    access: &[u8],
) -> Result<(), AuthError> {
    match read_access(vault, provider) {
        Ok(saved) if saved.expose() == access => Ok(()),
        Ok(_) | Err(AuthError::NotFound) => write_access(vault, provider, access),
        Err(e) => Err(e),
    }
}
pub(crate) fn clear_tokens(
    vault: &VaultService,
    provider: &str,
    profile: &mut Profile,
) -> Result<(), AuthError> {
    profile.tokens = None;
    write_profile(vault, provider, profile)?;
    match vault.delete_secret(&secret(&crate::AuthService::secret_name(provider))?) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == VaultErrorKind::SecretNotFound => Ok(()),
        Err(e) => Err(error(e)),
    }
}
pub(crate) fn host_id(vault: &VaultService, host_key: &str) -> Result<String, AuthError> {
    let mut hosts: BTreeMap<String, String> = read_json(vault, "oauth.hosts")?.unwrap_or_default();
    let fingerprint = format!("{:x}", Sha256::digest(host_key.as_bytes()));
    if let Some(id) = hosts.get(&fingerprint) {
        return Ok(id.clone());
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::Unavailable)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let id = format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    hosts.insert(fingerprint, id.clone());
    write_json(vault, "oauth.hosts", &hosts)?;
    Ok(id)
}
pub(crate) fn native_host_key() -> Result<String, AuthError> {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey("SOFTWARE\\Microsoft\\Cryptography")
            .and_then(|key| key.get_value::<String, _>("MachineGuid"))
            .map_err(|_| AuthError::Unavailable)
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/etc/machine-id").map_err(|_| AuthError::Unavailable)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(AuthError::Unavailable)
    }
}
