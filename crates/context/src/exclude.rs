//! Secret exclusion applied to indexing, search results, and context packs.
//!
//! Two layers, both fail closed:
//!
//! - **Names.** Files whose workspace-relative path looks credential-bearing are
//!   never read, indexed, returned by search, or put in a pack, even when asked for
//!   explicitly ([`is_secret_path`]).
//! - **Content.** A file that contains a private-key block is treated as a key file
//!   and excluded entirely; any other recognisable credential (tokens, passwords in
//!   assignments or URLs, bearer credentials) is redacted with
//!   [`aidex_events::redact_text`] before text is stored or packed, so incidental
//!   matches do not hide the rest of the file ([`classify_content`]).

use std::borrow::Cow;

use aidex_events::redact_text;

/// Directory names whose whole subtree is credential material.
const SECRET_DIRS: [&str; 4] = [".ssh", ".gnupg", ".aws", ".docker"];

/// Exact file names (case-insensitive).
const SECRET_NAMES: [&str; 11] = [
    ".npmrc",
    ".pypirc",
    ".netrc",
    "_netrc",
    ".pgpass",
    ".git-credentials",
    ".htpasswd",
    ".dockercfg",
    ".boto",
    ".s3cfg",
    "terraform.tfvars",
];

/// File-name prefixes (case-insensitive): `.env`, `.env.local`, `.envrc`, SSH key
/// pairs (public halves included), `credentials*`, `secrets.*`, OAuth client secrets.
const SECRET_PREFIXES: [&str; 8] = [
    ".env",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "credentials",
    "secrets.",
    "client_secret",
];

/// File extensions (case-insensitive).
const SECRET_EXTENSIONS: [&str; 13] = [
    "pem", "key", "p12", "pfx", "keystore", "jks", "ppk", "kdbx", "tfstate", "tfvars", "asc",
    "gpg", "keychain",
];

/// Whether a workspace-relative (`/`-separated) path names a likely secret file.
pub fn is_secret_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let mut components: Vec<&str> = lower.split('/').filter(|part| !part.is_empty()).collect();
    let Some(name) = components.pop() else {
        return false;
    };
    if components.iter().any(|dir| SECRET_DIRS.contains(dir)) || SECRET_DIRS.contains(&name) {
        return true;
    }
    if SECRET_NAMES.contains(&name)
        || SECRET_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    {
        return true;
    }
    name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty() && SECRET_EXTENSIONS.contains(&extension)
    })
}

/// Whether a directory name should be pruned from the walk entirely.
pub(crate) fn is_secret_dir_name(name: &str) -> bool {
    SECRET_DIRS
        .iter()
        .any(|dir| dir.eq_ignore_ascii_case(name))
}

/// What to do with a text file's content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentVerdict {
    /// Nothing credential-like was found.
    Clean,
    /// The file is key material (it contains a private-key block) and is excluded.
    KeyMaterial,
    /// Incidental credentials were found; they are redacted where text is kept.
    Redact,
}

/// Classify text content for indexing.
pub fn classify_content(text: &str) -> ContentVerdict {
    if contains_private_key(text) {
        ContentVerdict::KeyMaterial
    } else if matches!(redact_text(text), Cow::Owned(_)) {
        ContentVerdict::Redact
    } else {
        ContentVerdict::Clean
    }
}

/// Whether `text` contains a `-----BEGIN … PRIVATE KEY-----` (or PGP private key)
/// block header.
pub fn contains_private_key(text: &str) -> bool {
    const BEGIN: &[u8] = b"-----BEGIN ";
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(offset) = find(&bytes[from..], BEGIN) {
        let start = from + offset + BEGIN.len();
        let window = &bytes[start..(start + 64).min(bytes.len())];
        let label = find(window, b"-----").map_or(window, |stop| &window[..stop]);
        if find(label, b"PRIVATE KEY").is_some() {
            return true;
        }
        from = start;
    }
    false
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Redact credentials in text that will be stored or shown.
pub fn redact(text: &str) -> Cow<'_, str> {
    redact_text(text)
}

/// Whether `text` contains anything [`redact`] would replace.
pub fn contains_secret_like(text: &str) -> bool {
    contains_private_key(text) || matches!(redact_text(text), Cow::Owned(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_file_names_are_secret() {
        for path in [
            ".env",
            ".env.local",
            ".env.example",
            ".envrc",
            "app/.ENV.production",
            "certs/server.pem",
            "tls/server.KEY",
            "store.p12",
            "store.pfx",
            "release.keystore",
            "deploy/id_rsa",
            "deploy/id_ed25519.pub",
            "credentials",
            "config/credentials.json",
            ".aws/credentials",
            "home/.ssh/config",
            ".npmrc",
            ".pypirc",
            ".netrc",
            ".git-credentials",
            "secrets.yaml",
            "deploy/secrets.json",
            "client_secret_123.apps.json",
            "infra/prod.tfstate",
            "infra/prod.tfvars",
        ] {
            assert!(is_secret_path(path), "{path}");
        }
    }

    #[test]
    fn ordinary_names_are_not_secret() {
        for path in [
            "src/main.rs",
            "docs/keys.md",
            "src/secret_sharing.rs",
            "src/environment.ts",
            "key",
            ".pem",
            "README.md",
            "src/credential_helper_test.go",
            "lib/tokens.rs",
        ] {
            assert!(!is_secret_path(path), "{path}");
        }
    }

    #[test]
    fn private_key_blocks_mark_key_material() {
        let header = format!("-----BEGIN {}-----", "OPENSSH PRIVATE KEY");
        let text = format!("notes\n{header}\nAAAA\n-----END OPENSSH PRIVATE KEY-----\n");
        assert_eq!(classify_content(&text), ContentVerdict::KeyMaterial);
        let pgp = format!("-----BEGIN {}-----", "PGP PRIVATE KEY BLOCK");
        assert!(contains_private_key(&pgp));
        let public = "-----BEGIN PUBLIC KEY-----\nAAAA\n-----END PUBLIC KEY-----";
        assert!(!contains_private_key(public));
    }

    #[test]
    fn incidental_credentials_are_redacted_not_excluded() {
        let token = format!("{}{}", "ghp_", "FAKE0fake1FAKE2fake3FAKE4fake5");
        let text = format!("fn main() {{}}\nlet token = \"{token}\";\n");
        assert_eq!(classify_content(&text), ContentVerdict::Redact);
        assert!(!redact(&text).contains(&token));
        assert_eq!(
            classify_content("fn main() { println!(\"hi\"); }\n"),
            ContentVerdict::Clean
        );
    }
}
