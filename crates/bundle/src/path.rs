//! Validation for bundle-relative paths, versions, commits, and version directory names.
//!
//! The rules are stricter than any target filesystem so a bundle written on one host
//! extracts to the same names on NTFS, exFAT, FAT32, ext4, and APFS: ASCII only, no
//! traversal, no Windows device names, and no trailing dots or spaces (which Windows
//! silently strips).

use crate::{Target, layout::VERSIONS_DIR};

const MAX_PATH_LEN: usize = 240;
const MAX_SEGMENT_LEN: usize = 128;
const MAX_DEPTH: usize = 16;
const MAX_VERSION_LEN: usize = 64;
const RESERVED_DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validate a slash-separated path relative to a bundle directory.
pub fn validate_bundle_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path is empty".into());
    }
    if path.len() > MAX_PATH_LEN {
        return Err(format!("path `{path}` is longer than {MAX_PATH_LEN} bytes"));
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() > MAX_DEPTH {
        return Err(format!(
            "path `{path}` is nested deeper than {MAX_DEPTH} levels"
        ));
    }
    for segment in segments {
        validate_segment(segment).map_err(|reason| format!("path `{path}`: {reason}"))?;
    }
    Ok(())
}

/// Validate one path segment (a file or directory name).
pub(crate) fn validate_segment(segment: &str) -> Result<(), String> {
    if segment.is_empty() {
        return Err("empty path segment".into());
    }
    if segment == "." || segment == ".." {
        return Err("relative traversal segments are not allowed".into());
    }
    if segment.len() > MAX_SEGMENT_LEN {
        return Err(format!("segment is longer than {MAX_SEGMENT_LEN} bytes"));
    }
    if let Some(bad) = segment
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '@' | ' ')))
    {
        return Err(format!("character {bad:?} is not allowed in `{segment}`"));
    }
    if segment.starts_with(' ') || segment.ends_with(' ') || segment.ends_with('.') {
        return Err(format!(
            "`{segment}` starts or ends with a character Windows strips"
        ));
    }
    let stem = segment.split('.').next().unwrap_or(segment);
    if RESERVED_DEVICE_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        return Err(format!("`{segment}` is a reserved Windows device name"));
    }
    Ok(())
}

/// Validate a product version: starts with a digit, `[0-9A-Za-z.+-]`, at most 64 bytes.
/// SemVer pre-release (`-rc.1`) and build metadata (`+g1234`) are accepted.
pub fn validate_version(version: &str) -> Result<(), String> {
    let valid = !version.is_empty()
        && version.len() <= MAX_VERSION_LEN
        && version.starts_with(|c: char| c.is_ascii_digit())
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
        && !version.contains("..")
        && !version.ends_with('.');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "version `{version}` must start with a digit and use only [0-9A-Za-z.+-] (at most {MAX_VERSION_LEN} bytes)"
        ))
    }
}

/// Validate a full git commit id (SHA-1 or SHA-256 object name, lowercase hex).
pub fn validate_git_commit(commit: &str) -> Result<(), String> {
    let valid = matches!(commit.len(), 40 | 64)
        && commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "git commit `{commit}` must be a full lowercase hex object id"
        ))
    }
}

/// The default directory name of a bundle under `app/versions/`.
pub fn version_dir_name(version: &str, target: Target) -> String {
    format!("{version}-{target}")
}

/// Validate a pointer `dir` value (`versions/<name>`) and return `<name>`.
///
/// The name must be exactly one safe segment that does not start with `.`, so a
/// pointer can never reach outside `app/versions/` or into staging directories.
pub fn parse_pointer_dir(dir: &str) -> Result<&str, String> {
    let name = dir
        .strip_prefix(VERSIONS_DIR)
        .and_then(|rest| rest.strip_prefix('/'))
        .ok_or_else(|| format!("`{dir}` is not inside `{VERSIONS_DIR}/`"))?;
    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "`{dir}` must name a single directory inside `{VERSIONS_DIR}/`"
        ));
    }
    validate_segment(name).map_err(|reason| format!("`{dir}`: {reason}"))?;
    if name.starts_with('.') {
        return Err(format!("`{dir}` names a hidden directory"));
    }
    Ok(name)
}

/// The pointer `dir` value for a version directory name.
pub fn pointer_dir(name: &str) -> String {
    format!("{VERSIONS_DIR}/{name}")
}

/// `base` joined with a validated slash-separated relative path.
pub(crate) fn join_relative(base: &std::path::Path, relative: &str) -> std::path::PathBuf {
    relative
        .split('/')
        .fold(base.to_path_buf(), |path, segment| path.join(segment))
}

/// Case-folded form used to detect names that collide on case-insensitive filesystems.
pub(crate) fn fold(path: &str) -> String {
    path.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_paths_reject_traversal_and_unportable_names() {
        for good in [
            "bin/aidex.exe",
            "licenses/third-party/serde-1.0.219.txt",
            "desktop/AiDEX Desktop.exe",
            "docs/c++@notes.md",
        ] {
            validate_bundle_path(good).unwrap();
        }
        for bad in [
            "",
            "/bin/aidex",
            "bin//aidex",
            "bin/../aidex",
            "./bin/aidex",
            "bin\\aidex.exe",
            "C:/aidex",
            "bin/aidex.",
            "bin/aidex ",
            "bin/NUL.txt",
            "bin/con",
            "bin/caf\u{e9}",
            "bin/a*b",
        ] {
            assert!(
                validate_bundle_path(bad).is_err(),
                "{bad:?} must be rejected"
            );
        }
        assert!(validate_bundle_path("a/".repeat(20).trim_end_matches('/')).is_err());
    }

    #[test]
    fn versions_and_commits_are_validated() {
        for good in ["0.1.0", "1.2.3-rc.1", "0.1.0+g1a2b3c4", "2026.10.1"] {
            validate_version(good).unwrap();
        }
        for bad in [
            "",
            "v0.1.0",
            "0.1.0/x",
            "0..1",
            "0.1.",
            "0.1.0 beta",
            &"9".repeat(65),
        ] {
            assert!(validate_version(bad).is_err(), "{bad:?} must be rejected");
        }
        validate_git_commit(&"a".repeat(40)).unwrap();
        validate_git_commit(&"0".repeat(64)).unwrap();
        assert!(validate_git_commit(&"A".repeat(40)).is_err());
        assert!(validate_git_commit("abc123").is_err());
    }

    #[test]
    fn pointer_dirs_stay_inside_versions() {
        assert_eq!(
            parse_pointer_dir("versions/0.1.0-windows-x64").unwrap(),
            "0.1.0-windows-x64"
        );
        for bad in [
            "0.1.0-windows-x64",
            "versions",
            "versions/",
            "versions/../current.json",
            "versions/..",
            "versions/a/b",
            "versions/a\\b",
            "versions/.staging",
            "/versions/a",
            "../versions/a",
            "C:/app/versions/a",
            "versions/a.",
        ] {
            assert!(parse_pointer_dir(bad).is_err(), "{bad:?} must be rejected");
        }
        assert_eq!(pointer_dir("x"), "versions/x");
        assert_eq!(
            version_dir_name("0.1.0", Target::LinuxX64),
            "0.1.0-linux-x64"
        );
    }
}
