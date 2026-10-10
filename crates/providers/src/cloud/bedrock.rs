//! Converse wire mapping and AWS Signature Version 4 authentication.
use crate::*;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AwsCredential {
    access_key_id: String,
    secret_access_key: String,
    region: String,
    #[serde(default)]
    session_token: Option<String>,
}
impl Drop for AwsCredential {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.access_key_id.zeroize();
        self.secret_access_key.zeroize();
        if let Some(token) = &mut self.session_token {
            token.zeroize();
        }
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn hmac(key: &[u8], value: &str) -> Zeroizing<Vec<u8>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(value.as_bytes());
    Zeroizing::new(mac.finalize().into_bytes().to_vec())
}
pub(super) fn sign(
    url: &str,
    body: &[u8],
    secret: &SecretBytes,
) -> Result<Vec<(String, Zeroizing<String>)>, ProviderError> {
    let creds: AwsCredential = serde_json::from_slice(secret.expose()).map_err(|_| ProviderError::Credential(CredentialError::Unavailable { reason: "AWS vault credential must contain access_key_id, secret_access_key, region and optional session_token".into() }))?;
    if creds.access_key_id.is_empty()
        || creds.secret_access_key.is_empty()
        || creds.region.is_empty()
        || !creds
            .region
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !creds
            .access_key_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric())
        || creds
            .session_token
            .as_ref()
            .is_some_and(|s| s.bytes().any(|b| !b.is_ascii_graphic()))
    {
        return Err(ProviderError::Credential(CredentialError::Unavailable {
            reason: "invalid AWS credential fields".into(),
        }));
    }
    let timestamp = aidex_core::Timestamp::now();
    let timestamp = timestamp.as_str();
    let date: String = timestamp[..10].chars().filter(|c| *c != '-').collect();
    let time: String = timestamp[11..19].chars().filter(|c| *c != ':').collect();
    let amz_date = format!("{date}T{time}Z");
    let (_, remainder) = url.split_once("://").ok_or_else(super::protocol)?;
    let (host, path) = remainder.split_once('/').ok_or_else(super::protocol)?;
    // AWS Bedrock signs the path with a second URI encoding pass.
    let canonical_path = format!(
        "/{}",
        path.split('/')
            .map(super::requests::encode_segment)
            .collect::<Vec<_>>()
            .join("/")
    );
    let payload_hash = hex(&Sha256::digest(body));
    let mut canonical_headers = Zeroizing::new(format!(
        "content-type:application/json\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    ));
    let mut names = "content-type;host;x-amz-content-sha256;x-amz-date".to_owned();
    if let Some(token) = &creds.session_token {
        canonical_headers.push_str(&format!("x-amz-security-token:{token}\n"));
        names.push_str(";x-amz-security-token");
    }
    let canonical = Zeroizing::new(format!(
        "POST\n{canonical_path}\n\n{}\n{names}\n{payload_hash}",
        canonical_headers.as_str()
    ));
    let scope = format!("{date}/{}/bedrock/aws4_request", creds.region);
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex(&Sha256::digest(canonical.as_bytes()))
    );
    let initial = Zeroizing::new(format!("AWS4{}", creds.secret_access_key));
    let day_key = hmac(initial.as_bytes(), &date);
    let region_key = hmac(&day_key, &creds.region);
    let service_key = hmac(&region_key, "bedrock");
    let signing_key = hmac(&service_key, "aws4_request");
    let signature = hex(&hmac(&signing_key, &to_sign));
    let mut headers = vec![
        (
            "authorization".into(),
            Zeroizing::new(format!(
                "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={names}, Signature={signature}",
                creds.access_key_id
            )),
        ),
        ("x-amz-date".into(), Zeroizing::new(amz_date)),
        ("x-amz-content-sha256".into(), Zeroizing::new(payload_hash)),
    ];
    if let Some(token) = &creds.session_token {
        headers.push(("x-amz-security-token".into(), Zeroizing::new(token.clone())));
    }
    Ok(headers)
}

pub(super) fn request(r: &ChatRequest) -> Result<(String, Value), ProviderError> {
    if r.response_format.is_some() {
        return Err(ProviderError::InvalidRequest(
            "Bedrock constrained output is not supported by this adapter".into(),
        ));
    }
    let mut system = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    let mut previous_was_tool = false;
    for m in &r.messages {
        if m.role == Role::System {
            system.push(json!({"text":m.content}));
            previous_was_tool = false;
            continue;
        }
        let mut content = Vec::new();
        if m.role == Role::Tool {
            content.push(
                json!({"toolResult":{"toolUseId":m.tool_call_id,"content":[{"text":m.content}]}}),
            );
        } else {
            if !m.content.is_empty() {
                content.push(json!({"text":m.content}));
            }
            for call in &m.tool_calls {
                let args: Value = serde_json::from_str(&call.arguments).map_err(|_| {
                    ProviderError::InvalidRequest("Bedrock tool arguments require JSON".into())
                })?;
                content
                    .push(json!({"toolUse":{"toolUseId":call.id,"name":call.name,"input":args}}));
            }
        }
        if m.role == Role::Tool && previous_was_tool {
            messages
                .last_mut()
                .and_then(|v| v["content"].as_array_mut())
                .ok_or_else(super::protocol)?
                .extend(content);
        } else {
            messages.push(json!({"role":if m.role == Role::Assistant { "assistant" } else { "user" },"content":content}));
        }
        previous_was_tool = m.role == Role::Tool;
    }
    let mut body = json!({"messages":messages,"inferenceConfig":{}});
    if !system.is_empty() {
        body["system"] = json!(system);
    }
    if let Some(max) = r.max_tokens {
        body["inferenceConfig"]["maxTokens"] = json!(max);
    }
    if let Some(t) = r.temperature {
        body["inferenceConfig"]["temperature"] = json!(t);
    }
    if !r.stop.is_empty() {
        body["inferenceConfig"]["stopSequences"] = json!(r.stop);
    }
    if !r.tools.is_empty() {
        if r.tool_choice == ToolChoice::None {
            return Err(ProviderError::InvalidRequest(
                "Bedrock does not support tool_choice none; omit tools".into(),
            ));
        }
        body["toolConfig"] = json!({"tools":r.tools.iter().map(|t| json!({"toolSpec":{"name":t.name,"description":t.description,"inputSchema":{"json":t.parameters}}})).collect::<Vec<_>>(),"toolChoice":match &r.tool_choice { ToolChoice::Auto => json!({"auto":{}}), ToolChoice::Required => json!({"any":{}}), ToolChoice::Tool { name } => json!({"tool":{"name":name}}), _ => unreachable!() }});
    }
    Ok((
        format!(
            "model/{}/converse-stream",
            super::requests::encode_segment(r.model.as_str())
        ),
        body,
    ))
}
