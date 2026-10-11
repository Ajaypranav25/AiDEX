//! Drafting drive skills: validation and `SKILL.md` rendering.

use std::borrow::Cow;

use aidex_events::redact_text;
use serde::{Deserialize, Serialize};

use crate::{
    SkillError,
    manifest::{MAX_BODY_BYTES, MAX_DESCRIPTION_CHARS, MAX_VERSION_CHARS},
    name::is_skill_name,
};

/// A drive skill as `skill.save` takes it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillDraft {
    pub name: String,
    pub description: String,
    /// The Markdown instructions.
    pub body: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// A YAML double-quoted scalar (the text has no control characters).
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        if matches!(ch, '"' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Validate `draft` and render its `SKILL.md`. Text that looks like it holds a
/// credential is refused: skills are stored unencrypted on the drive.
pub fn render_skill_file(draft: &SkillDraft) -> Result<String, SkillError> {
    if !is_skill_name(&draft.name) {
        return Err(SkillError::Invalid(
            "`name` must be 1-64 lowercase letters, digits, or `-`, starting with a letter or digit"
                .into(),
        ));
    }
    let description = draft.description.trim();
    if description.is_empty() {
        return Err(SkillError::Invalid("`description` is required".into()));
    }
    if description.chars().count() > MAX_DESCRIPTION_CHARS
        || description.chars().any(char::is_control)
    {
        return Err(SkillError::Invalid(format!(
            "`description` must be one line of at most {MAX_DESCRIPTION_CHARS} characters"
        )));
    }
    let version = draft
        .version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty());
    if let Some(version) = version
        && (version.chars().count() > MAX_VERSION_CHARS
            || !version
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '+' | '_')))
    {
        return Err(SkillError::Invalid(format!(
            "`version` must be at most {MAX_VERSION_CHARS} letters, digits, `.`, `-`, `+`, or `_`"
        )));
    }
    let body = draft.body.replace("\r\n", "\n");
    let body = body.trim_matches('\n').trim_end();
    if body.trim().is_empty() {
        return Err(SkillError::Invalid(
            "the skill needs instructions (`body`)".into(),
        ));
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(SkillError::Invalid(format!(
            "the instructions are larger than {} KiB",
            MAX_BODY_BYTES / 1024
        )));
    }
    if body.contains('\0') {
        return Err(SkillError::Invalid(
            "the instructions contain a NUL character".into(),
        ));
    }
    for text in [description, body] {
        if matches!(redact_text(text), Cow::Owned(_)) {
            return Err(SkillError::CredentialDetected);
        }
    }
    let mut file = format!(
        "---\nname: {}\ndescription: {}\n",
        draft.name,
        quoted(description)
    );
    if let Some(version) = version {
        file.push_str(&format!("version: {}\n", quoted(version)));
    }
    file.push_str("---\n\n");
    file.push_str(body);
    file.push('\n');
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SkillDocument, SkillManifest};

    fn draft(description: &str, body: &str) -> SkillDraft {
        SkillDraft {
            name: "release-notes".into(),
            description: description.into(),
            body: body.into(),
            version: Some("1.2.0".into()),
        }
    }

    #[test]
    fn rendered_files_parse_back_to_the_draft() {
        let text = render_skill_file(&draft(
            "  Write \"release\" notes: C:\\path, # not a comment  ",
            "\r\n# Steps\r\n\r\n1. Read the log\r\n---\r\nmore\r\n",
        ))
        .unwrap();
        assert!(text.starts_with("---\nname: release-notes\n"), "{text}");
        let document = SkillDocument::parse(&text).unwrap();
        let manifest =
            SkillManifest::from_front_matter(document.front_matter.as_ref(), None).unwrap();
        assert_eq!(manifest.name, "release-notes");
        assert_eq!(
            manifest.description,
            "Write \"release\" notes: C:\\path, # not a comment"
        );
        assert_eq!(manifest.version.as_deref(), Some("1.2.0"));
        assert_eq!(document.body, "# Steps\n\n1. Read the log\n---\nmore");
    }

    #[test]
    fn drafts_are_validated() {
        let token = format!("{}{}", "ghp_", "FAKE0fake1FAKE2fake3FAKE4fake5");
        for (bad, reason) in [
            (
                SkillDraft {
                    name: "Bad Name".into(),
                    ..draft("d", "b")
                },
                "invalid_skill",
            ),
            (draft(" ", "b"), "invalid_skill"),
            (draft("two\nlines", "b"), "invalid_skill"),
            (draft(&"x".repeat(1025), "b"), "invalid_skill"),
            (draft("d", "  \n "), "invalid_skill"),
            (draft("d", &"x".repeat(MAX_BODY_BYTES + 1)), "invalid_skill"),
            (
                SkillDraft {
                    version: Some("1 0".into()),
                    ..draft("d", "b")
                },
                "invalid_skill",
            ),
            (draft("d", &format!("use {token}")), "credential_detected"),
            (draft(&format!("key {token}"), "b"), "credential_detected"),
        ] {
            let error = render_skill_file(&bad).unwrap_err();
            assert_eq!(error.reason(), reason, "{bad:?}");
            assert!(!error.to_string().contains(&token));
        }
    }
}
