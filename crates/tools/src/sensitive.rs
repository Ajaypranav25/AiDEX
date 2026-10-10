//! A conservative name-based hint for credential-bearing files.
//!
//! This is a hint for policy (`PathClass::Sensitive`) and for skipping such files in
//! `fs.search`; it is not a secret scanner. The broker decides the final class.

/// Directory names whose contents are treated as sensitive.
const SENSITIVE_DIRS: [&str; 4] = [".ssh", ".aws", ".gnupg", ".docker"];

/// Exact file names (case-insensitive).
const SENSITIVE_NAMES: [&str; 12] = [
    ".env",
    ".netrc",
    "_netrc",
    ".npmrc",
    ".pypirc",
    ".pgpass",
    ".git-credentials",
    ".dockercfg",
    "credentials",
    "credentials.json",
    "terraform.tfvars",
    ".htpasswd",
];

/// File extensions (case-insensitive).
const SENSITIVE_EXTENSIONS: [&str; 9] = [
    "pem", "key", "p12", "pfx", "jks", "keystore", "kdbx", "tfstate", "ppk",
];

/// Private SSH key file name prefixes (`.pub` files are public).
const SSH_KEY_PREFIXES: [&str; 4] = ["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"];

/// Template suffixes that mark `.env.*` files as examples, not secrets.
const ENV_TEMPLATES: [&str; 4] = ["example", "sample", "template", "dist"];

/// Whether a workspace-relative path (`/`-separated) names a likely credential file.
pub fn looks_sensitive(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let mut components: Vec<&str> = lower.split('/').filter(|part| !part.is_empty()).collect();
    let Some(name) = components.pop() else {
        return false;
    };
    if components.iter().any(|dir| SENSITIVE_DIRS.contains(dir)) {
        return true;
    }
    if SENSITIVE_NAMES.contains(&name) {
        return true;
    }
    if let Some(variant) = name.strip_prefix(".env.") {
        return !ENV_TEMPLATES.contains(&variant);
    }
    if name.starts_with("client_secret") && name.ends_with(".json") {
        return true;
    }
    if SSH_KEY_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
        && !name.ends_with(".pub")
    {
        return true;
    }
    name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty() && SENSITIVE_EXTENSIONS.contains(&extension)
    })
}

#[cfg(test)]
mod tests {
    use super::looks_sensitive;

    #[test]
    fn recognises_common_credential_files() {
        for path in [
            ".env",
            "app/.env.production",
            "certs/server.PEM",
            "deploy/id_ed25519",
            "home/.ssh/config",
            ".aws/credentials",
            "client_secret_123.json",
            "infra/prod.tfstate",
        ] {
            assert!(looks_sensitive(path), "{path}");
        }
    }

    #[test]
    fn leaves_ordinary_and_template_files_alone() {
        for path in [
            "src/main.rs",
            ".env.example",
            "deploy/id_ed25519.pub",
            "docs/keys.md",
            "key",
            ".pem",
            "README",
        ] {
            assert!(!looks_sensitive(path), "{path}");
        }
    }
}
