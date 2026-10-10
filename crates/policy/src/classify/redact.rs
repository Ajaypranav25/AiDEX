//! Best-effort removal of credentials from command lines before display or audit.

/// Replacement text for removed values.
pub const REDACTED: &str = "[REDACTED]";

/// Options whose following word is a secret value.
const SECRET_OPTIONS: [&str; 22] = [
    "--password",
    "--passwd",
    "--pass",
    "--token",
    "--api-key",
    "--apikey",
    "--secret",
    "--client-secret",
    "--access-token",
    "--auth-token",
    "--oauth2-bearer",
    "--bearer",
    "--auth",
    "--key-password",
    "--private-key-passphrase",
    "-password",
    "-token",
    "-apikey",
    "-accesstoken",
    "-pass",
    "-secret",
    "-credential",
];

/// Prefixes of well-known credential formats and the minimum total length that
/// makes a match a credential rather than ordinary text.
const TOKEN_PREFIXES: [(&str, usize); 20] = [
    ("github_pat_", 30),
    ("ghp_", 30),
    ("gho_", 30),
    ("ghu_", 30),
    ("ghs_", 30),
    ("ghr_", 30),
    ("glpat-", 20),
    ("xoxb-", 20),
    ("xoxp-", 20),
    ("xoxa-", 20),
    ("xoxs-", 20),
    ("sk_live_", 20),
    ("sk_test_", 20),
    ("rk_live_", 20),
    ("sk-", 24),
    ("AKIA", 20),
    ("ASIA", 20),
    ("AIza", 30),
    ("hf_", 30),
    ("npm_", 30),
];

#[derive(Clone, Copy, Eq, PartialEq)]
enum Pending {
    None,
    Value,
    UserPassword,
}

/// Return `line` with likely credentials replaced by [`REDACTED`].
///
/// Removes values after secret-bearing options (`--password`, `--token`, `-u
/// user:pass`), `Authorization`/API-key header values, secret-named assignments
/// and query parameters (`GITHUB_TOKEN=...`, `?access_token=...`), URL user-info,
/// and well-known token formats (GitHub, GitLab, Slack, Stripe, OpenAI-style,
/// AWS access key ids, JWTs). Whitespace and quoting are preserved. This is a
/// heuristic for display and audit text; it is not a guarantee that a free-form
/// command contains no secret, so callers must still keep raw commands out of
/// logs and model context.
pub fn redact_command(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut pending = Pending::None;
    let mut start = 0;
    for (index, ch) in line.char_indices() {
        if ch.is_whitespace() {
            if start < index {
                pending = redact_token(&line[start..index], pending, &mut out);
            }
            out.push(ch);
            start = index + ch.len_utf8();
        }
    }
    if start < line.len() {
        redact_token(&line[start..], pending, &mut out);
    }
    out
}

fn redact_token(token: &str, pending: Pending, out: &mut String) -> Pending {
    let core_start = token
        .find(|ch: char| !matches!(ch, '"' | '\'' | '('))
        .unwrap_or(token.len());
    let core_end = token
        .rfind(|ch: char| !matches!(ch, '"' | '\'' | ')' | ';' | ','))
        .map_or(core_start, |index| index + 1)
        .max(core_start);
    let (prefix, core, suffix) = (
        &token[..core_start],
        &token[core_start..core_end],
        &token[core_end..],
    );
    out.push_str(prefix);
    let scheme = matches!(
        core.to_ascii_lowercase().as_str(),
        "bearer" | "basic" | "token" | "digest"
    );
    let next = match pending {
        // `Authorization: Bearer <value>`: keep the scheme word, redact what follows.
        Pending::Value if scheme => {
            out.push_str(core);
            Pending::Value
        }
        Pending::Value if !core.is_empty() => {
            out.push_str(REDACTED);
            Pending::None
        }
        Pending::UserPassword => {
            match core.split_once(':') {
                Some((user, _)) => {
                    out.push_str(user);
                    out.push(':');
                    out.push_str(REDACTED);
                }
                None => out.push_str(core),
            }
            Pending::None
        }
        _ => {
            let (text, next) = redact_core(core);
            out.push_str(&text);
            next
        }
    };
    out.push_str(suffix);
    next
}

fn redact_core(core: &str) -> (String, Pending) {
    let lower = core.to_ascii_lowercase();
    if SECRET_OPTIONS.contains(&lower.as_str()) {
        return (core.to_owned(), Pending::Value);
    }
    if lower == "-u" || lower == "--user" || lower == "--proxy-user" {
        return (core.to_owned(), Pending::UserPassword);
    }
    if lower == "bearer" || lower == "basic" || lower == "token" {
        return (core.to_owned(), Pending::Value);
    }
    if let Some(name) = lower.strip_suffix(':') {
        if is_secret_name(name) || name == "authorization" || name == "cookie" {
            return (core.to_owned(), Pending::Value);
        }
    }
    if core.contains("://") {
        return (redact_known_tokens(&redact_url(core)), Pending::None);
    }
    if let Some((name, value)) = core.split_once('=') {
        if !value.is_empty() && is_secret_name(name) {
            return (format!("{name}={REDACTED}"), Pending::None);
        }
    }
    (redact_known_tokens(core), Pending::None)
}

fn is_secret_name(name: &str) -> bool {
    let name = name
        .trim_start_matches('-')
        .trim_start_matches("$env:")
        .to_ascii_lowercase();
    const MARKERS: [&str; 13] = [
        "token",
        "secret",
        "password",
        "passwd",
        "pwd",
        "apikey",
        "api_key",
        "api-key",
        "auth",
        "credential",
        "private_key",
        "access_key",
        "signature",
    ];
    MARKERS.iter().any(|marker| name.contains(marker))
        || name == "key"
        || name.ends_with("_key")
        || name.ends_with("-key")
        || name == "sig"
}

fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_owned();
    };
    let authority_start = scheme_end + 3;
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |index| authority_start + index);
    let authority = &url[authority_start..authority_end];
    let mut out = url[..authority_start].to_owned();
    match authority.rsplit_once('@') {
        Some((userinfo, host)) => {
            match userinfo.split_once(':') {
                Some((user, _)) => {
                    out.push_str(user);
                    out.push(':');
                    out.push_str(REDACTED);
                }
                None => out.push_str(REDACTED),
            }
            out.push('@');
            out.push_str(host);
        }
        None => out.push_str(authority),
    }
    let rest = &url[authority_end..];
    match rest.split_once('?') {
        Some((path, query)) => {
            out.push_str(path);
            out.push('?');
            let pairs: Vec<String> = query
                .split('&')
                .map(|pair| match pair.split_once('=') {
                    Some((name, value)) if !value.is_empty() && is_secret_name(name) => {
                        format!("{name}={REDACTED}")
                    }
                    _ => pair.to_owned(),
                })
                .collect();
            out.push_str(&pairs.join("&"));
        }
        None => out.push_str(rest),
    }
    out
}

fn redact_known_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        let boundary = text[..index]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());
        if boundary {
            if let Some(length) = credential_length(rest) {
                out.push_str(REDACTED);
                index += length;
                continue;
            }
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

/// Length of a credential at the start of `text`, if one is there.
fn credential_length(text: &str) -> Option<usize> {
    let run = text
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')))
        .unwrap_or(text.len());
    let candidate = &text[..run];
    let prefixed = TOKEN_PREFIXES
        .iter()
        .any(|(prefix, minimum)| candidate.starts_with(prefix) && candidate.len() >= *minimum);
    let jwt = candidate.starts_with("eyJ")
        && candidate.matches('.').count() == 2
        && candidate.len() >= 30;
    (prefixed || jwt).then_some(run)
}
