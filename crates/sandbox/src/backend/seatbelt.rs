//! Seatbelt (SBPL) profile generation for macOS `sandbox-exec`. Pure string building,
//! compiled everywhere so it is unit tested on every platform.
//!
//! Paths are never spliced into the profile text: each writable root is passed as a
//! `-D AIDEX_RW_<n>=<path>` parameter and referenced with `(param "AIDEX_RW_<n>")`.

/// The system `sandbox-exec` binary.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Device paths a write-confined process may still write.
const WRITABLE_DEVICES: &[&str] = &[
    "/dev/null",
    "/dev/zero",
    "/dev/dtracehelper",
    "/dev/stdout",
    "/dev/stderr",
];

/// A profile plus the `-D` parameters it references.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Profile {
    pub text: String,
    pub params: Vec<(String, String)>,
}

/// Build a profile that allows everything except what is explicitly confined.
///
/// Later SBPL rules take precedence over earlier ones, so the writable roots listed
/// after the blanket `deny file-write*` re-allow writes beneath them only.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn profile(
    writable_roots: &[String],
    confine_writes: bool,
    deny_network: bool,
) -> Profile {
    let mut text = String::from("(version 1)\n(allow default)\n");
    let mut params = Vec::new();
    if confine_writes {
        text.push_str("(deny file-write*)\n(allow file-write*\n");
        for (index, root) in writable_roots.iter().enumerate() {
            let name = format!("AIDEX_RW_{index}");
            text.push_str(&format!("    (subpath (param \"{name}\"))\n"));
            params.push((name, root.clone()));
        }
        for device in WRITABLE_DEVICES {
            text.push_str(&format!("    (literal \"{device}\")\n"));
        }
        text.push_str("    (regex #\"^/dev/fd/[0-9]+$\"))\n");
    }
    if deny_network {
        text.push_str("(deny network*)\n");
    }
    Profile { text, params }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_confinement_uses_parameters_not_inline_paths() {
        let roots = vec![
            "/Users/me/work \"quoted\"".to_owned(),
            "/private/tmp/x".to_owned(),
        ];
        let profile = profile(&roots, true, false);
        assert!(
            profile
                .text
                .starts_with("(version 1)\n(allow default)\n(deny file-write*)")
        );
        assert!(profile.text.contains("(subpath (param \"AIDEX_RW_0\"))"));
        assert!(profile.text.contains("(subpath (param \"AIDEX_RW_1\"))"));
        assert!(!profile.text.contains("/Users/me"));
        assert!(!profile.text.contains("network"));
        assert_eq!(
            profile.params,
            vec![
                ("AIDEX_RW_0".to_owned(), roots[0].clone()),
                ("AIDEX_RW_1".to_owned(), roots[1].clone()),
            ]
        );
    }

    #[test]
    fn network_only_profile_leaves_files_alone() {
        let profile = profile(&[], false, true);
        assert_eq!(
            profile.text,
            "(version 1)\n(allow default)\n(deny network*)\n"
        );
        assert!(profile.params.is_empty());
    }
}
