//! Supported OpenAI OSS authorization-code + PKCE protocol. No ambient credentials.
use crate::{AuthError, storage::Profile};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Read,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};
pub(crate) const DISCOVERY: &str = "https://auth.openai.com/.well-known/openid-configuration";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const MAX_RESPONSE: u64 = 1024 * 1024;

#[derive(Deserialize)]
pub(crate) struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub revocation_endpoint: Option<String>,
}
#[derive(Clone, Deserialize, Serialize, Zeroize, ZeroizeOnDrop)]
pub(crate) struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_at: u64,
    pub earliest_refresh_at: Option<u64>,
    pub scope: String,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
pub(crate) struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    scope: String,
    #[zeroize(skip)]
    earliest_refresh_at: Option<serde_json::Value>,
}
impl TokenResponse {
    fn validate(&self) -> Result<(), AuthError> {
        if self.access_token.is_empty()
            || self.expires_in == 0
            || !self.token_type.eq_ignore_ascii_case("bearer")
            || self
                .refresh_token
                .as_ref()
                .is_some_and(|token| token.is_empty())
        {
            return Err(AuthError::Unavailable);
        }
        let scopes: Vec<_> = self.scope.split_whitespace().collect();
        if !scopes.contains(&"chatgpt.tokens.use.direct") || !scopes.contains(&"resource.invoke") {
            return Err(AuthError::PlanPermission);
        }
        Ok(())
    }
    pub fn into_tokens(self, previous: Option<&Tokens>) -> Result<Tokens, AuthError> {
        self.validate()?;
        let earliest = match self.earliest_refresh_at.as_ref() {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::Number(n)) => {
                let seconds = n
                    .as_f64()
                    .filter(|seconds| {
                        seconds.is_finite() && *seconds >= 0.0 && *seconds < u64::MAX as f64
                    })
                    .ok_or(AuthError::Unavailable)?;
                Some(seconds.ceil() as u64)
            }
            Some(serde_json::Value::String(s)) => {
                let date =
                    chrono::DateTime::parse_from_rfc3339(s).map_err(|_| AuthError::Unavailable)?;
                let seconds: u64 = date
                    .timestamp()
                    .try_into()
                    .map_err(|_| AuthError::Unavailable)?;
                Some(seconds.saturating_add(u64::from(date.timestamp_subsec_nanos() > 0)))
            }
            Some(_) => return Err(AuthError::Unavailable),
        };
        Ok(Tokens {
            access_token: self.access_token.clone(),
            refresh_token: self
                .refresh_token
                .clone()
                .or_else(|| previous.and_then(|p| p.refresh_token.clone())),
            id_token: if let Some(previous) = previous {
                previous.id_token.clone()
            } else {
                self.id_token.clone()
            },
            expires_at: now()
                .checked_add(self.expires_in)
                .ok_or(AuthError::Unavailable)?,
            earliest_refresh_at: earliest,
            scope: self.scope.clone(),
        })
    }
}
#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct Attempt {
    pub url: String,
    pub verifier: String,
    pub nonce: String,
    pub callback: String,
    state: String,
    client: Option<String>,
}
impl Attempt {
    pub fn new(
        callback: String,
        profile: Option<&Profile>,
        host_id: &str,
        endpoints: &Discovery,
    ) -> Result<Self, AuthError> {
        let verifier = random_string()?;
        let state = random_string()?;
        let nonce = random_string()?;
        let client = profile.map(|p| p.client_id.clone());
        let mut url =
            Url::parse(&endpoints.authorization_endpoint).map_err(|_| AuthError::Unavailable)?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair(
                    "client_id",
                    client.as_deref().unwrap_or("dynamic_agent_client"),
                )
                .append_pair("ext_agent_host_id", host_id)
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &callback)
                .append_pair("scope", SCOPE)
                .append_pair("resource", RESOURCE)
                .append_pair("state", &state)
                .append_pair("nonce", &nonce)
                .append_pair("code_challenge_method", "S256")
                .append_pair(
                    "code_challenge",
                    &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
                );
            if client.is_none() {
                query.append_pair("agent_name_hint", "AiDEX");
            }
            if let Some(email) = profile.and_then(|p| p.email.as_deref()) {
                query.append_pair("login_hint", email);
            }
        }
        // A retained ID token stays encrypted. id_token_hint would leak it to RPC/UI.
        Ok(Self {
            url: url.into(),
            verifier,
            nonce,
            callback,
            state,
            client,
        })
    }
    pub fn callback(&self, url: &Url) -> Result<(String, Zeroizing<String>), AuthError> {
        let mut params = HashMap::new();
        for (key, value) in url.query_pairs() {
            if params
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Err(AuthError::CallbackInvalid);
            }
        }
        if params.get("state").map(String::as_str) != Some(&self.state) {
            return Err(AuthError::CallbackInvalid);
        }
        if let Some(error) = params.get("error") {
            return Err(if error == "access_denied" {
                AuthError::ConsentDenied
            } else {
                AuthError::CallbackInvalid
            });
        }
        let client = match self.client.as_ref() {
            Some(client) => {
                if params
                    .get("client_id")
                    .is_some_and(|returned| returned != client)
                {
                    return Err(AuthError::CallbackInvalid);
                }
                client.clone()
            }
            None => params
                .get("client_id")
                .filter(|client| {
                    client.starts_with("oaiapp_")
                        && client.len() > 7
                        && client.len() <= 256
                        && client
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
                })
                .cloned()
                .ok_or(AuthError::CallbackInvalid)?,
        };
        let code = params
            .remove("code")
            .filter(|code| !code.is_empty() && code.len() <= 4096)
            .ok_or(AuthError::CallbackInvalid)?;
        Ok((client, Zeroizing::new(code)))
    }
}
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub(crate) fn random_string() -> Result<String, AuthError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::Unavailable)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn read_json<T: DeserializeOwned>(response: reqwest::blocking::Response) -> Result<T, AuthError> {
    let mut bytes = Zeroizing::new(Vec::new());
    response
        .take(MAX_RESPONSE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AuthError::Unavailable)?;
    if bytes.len() as u64 > MAX_RESPONSE {
        return Err(AuthError::Unavailable);
    }
    serde_json::from_slice(&bytes).map_err(|_| AuthError::Unavailable)
}
pub(crate) fn discover(
    http: &reqwest::blocking::Client,
    discovery: &str,
    allowed: &dyn Fn() -> bool,
) -> Result<Discovery, AuthError> {
    network(allowed)?;
    let response = http
        .get(discovery)
        .send()
        .map_err(|_| AuthError::Unavailable)?;
    if !response.status().is_success() {
        return Err(AuthError::Unavailable);
    }
    let result: Discovery = read_json(response)?;
    let expected = Url::parse(discovery).map_err(|_| AuthError::Unavailable)?;
    #[cfg(test)]
    let fixture = expected.scheme() == "http" && expected.host_str() == Some("127.0.0.1");
    #[cfg(not(test))]
    let fixture = false;
    let issuer = Url::parse(&result.issuer).map_err(|_| AuthError::Unavailable)?;
    if (!fixture && result.issuer != "https://auth.openai.com")
        || (fixture && issuer.origin() != expected.origin())
    {
        return Err(AuthError::Unavailable);
    }
    for endpoint in [
        &result.authorization_endpoint,
        &result.token_endpoint,
        &result.jwks_uri,
    ]
    .into_iter()
    .chain(result.revocation_endpoint.as_ref())
    {
        let parsed = Url::parse(endpoint).map_err(|_| AuthError::Unavailable)?;
        if parsed.origin() != issuer.origin()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(AuthError::Unavailable);
        }
    }
    Ok(result)
}
fn token_request(
    http: &reqwest::blocking::Client,
    endpoints: &Discovery,
    form: &[(&str, &str)],
    refresh: bool,
    allowed: &dyn Fn() -> bool,
) -> Result<TokenResponse, AuthError> {
    network(allowed)?;
    let response = http
        .post(&endpoints.token_endpoint)
        .form(form)
        .send()
        .map_err(|_| AuthError::TokenTransport)?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        if refresh {
            let body: serde_json::Value = read_json(response)?;
            if matches!(
                body.get("error").and_then(|value| value.as_str()),
                Some(
                    "invalid_grant"
                        | "invalid_refresh_token"
                        | "refresh_token_expired"
                        | "refresh_token_invalidated"
                        | "refresh_token_reused"
                )
            ) {
                return Err(AuthError::Reauthorize);
            }
        }
        return Err(AuthError::TokenRejected(status));
    }
    let tokens: TokenResponse = read_json(response).map_err(|_| AuthError::TokenResponseInvalid)?;
    tokens.validate().map_err(|error| {
        if error == AuthError::Unavailable {
            AuthError::TokenResponseInvalid
        } else {
            error
        }
    })?;
    Ok(tokens)
}
pub(crate) fn exchange(
    http: &reqwest::blocking::Client,
    endpoints: &Discovery,
    client: &str,
    code: &str,
    verifier: &str,
    callback: &str,
    allowed: &dyn Fn() -> bool,
) -> Result<TokenResponse, AuthError> {
    token_request(
        http,
        endpoints,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", callback),
            ("resource", RESOURCE),
        ],
        false,
        allowed,
    )
}
pub(crate) fn refresh(
    http: &reqwest::blocking::Client,
    endpoints: &Discovery,
    client: &str,
    refresh_token: &str,
    allowed: &dyn Fn() -> bool,
) -> Result<TokenResponse, AuthError> {
    token_request(
        http,
        endpoints,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client),
            ("refresh_token", refresh_token),
            ("resource", RESOURCE),
        ],
        true,
        allowed,
    )
}
#[derive(Deserialize)]
pub(crate) struct Identity {
    pub sub: String,
    pub email: Option<String>,
    nonce: String,
    aud: serde_json::Value,
    azp: Option<String>,
}
pub(crate) fn validate_identity(
    http: &reqwest::blocking::Client,
    endpoints: &Discovery,
    tokens: &TokenResponse,
    client: &str,
    nonce: &str,
    allowed: &dyn Fn() -> bool,
) -> Result<Identity, AuthError> {
    let token = tokens
        .id_token
        .as_deref()
        .filter(|token| !token.is_empty())
        .ok_or(AuthError::IdentityInvalid)?;
    let header = decode_header(token).map_err(|_| AuthError::IdentityInvalid)?;
    if header.alg != Algorithm::RS256 {
        return Err(AuthError::IdentityInvalid);
    }
    let kid = header.kid.ok_or(AuthError::IdentityInvalid)?;
    network(allowed)?;
    let response = http
        .get(&endpoints.jwks_uri)
        .send()
        .map_err(|_| AuthError::IdentityKeysUnavailable)?;
    if !response.status().is_success() {
        return Err(AuthError::IdentityKeysUnavailable);
    }
    let jwks: JwkSet = read_json(response).map_err(|_| AuthError::IdentityKeysUnavailable)?;
    let key = DecodingKey::from_jwk(jwks.find(&kid).ok_or(AuthError::IdentityInvalid)?)
        .map_err(|_| AuthError::IdentityInvalid)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&endpoints.issuer]);
    validation.set_audience(&[client]);
    validation.leeway = 0;
    validation.validate_nbf = true;
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    let identity = decode::<Identity>(token, &key, &validation)
        .map_err(|_| AuthError::IdentityInvalid)?
        .claims;
    if identity.nonce != nonce
        || identity.sub.is_empty()
        || identity.sub.len() > 256
        || identity.sub.chars().any(char::is_control)
        || identity
            .email
            .as_ref()
            .is_some_and(|email| email.len() > 320 || email.chars().any(char::is_control))
        || identity.aud.as_array().is_some_and(|aud| aud.len() > 1)
            && identity.azp.as_deref() != Some(client)
    {
        return Err(AuthError::IdentityInvalid);
    }
    Ok(identity)
}
pub(crate) fn revoke(
    http: &reqwest::blocking::Client,
    endpoints: &Discovery,
    client: &str,
    refresh_token: &str,
    allowed: &dyn Fn() -> bool,
) -> Result<(), AuthError> {
    let endpoint = endpoints
        .revocation_endpoint
        .as_ref()
        .ok_or(AuthError::RemoteRevocationUnconfirmed)?;
    for attempt in 0..3 {
        network(allowed)?;
        match http
            .post(endpoint)
            .form(&[
                ("token", refresh_token),
                ("token_type_hint", "refresh_token"),
                ("client_id", client),
            ])
            .send()
        {
            Ok(response) if response.status().as_u16() == 200 => return Ok(()),
            Ok(response) if !response.status().is_server_error() => break,
            _ => {}
        }
        if attempt < 2 {
            std::thread::sleep(std::time::Duration::from_millis(100 * (1 << attempt)));
        }
    }
    Err(AuthError::RemoteRevocationUnconfirmed)
}

pub(crate) fn network(allowed: &dyn Fn() -> bool) -> Result<(), AuthError> {
    if allowed() {
        Ok(())
    } else {
        Err(AuthError::NetworkOffline)
    }
}
