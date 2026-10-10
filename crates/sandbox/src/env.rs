//! Environment allowlisting and program resolution for a cleared child environment.

use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use crate::{SandboxError, paths};

/// Parent variables copied by default. None of them carries credentials.
#[cfg(windows)]
pub const DEFAULT_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SystemRoot",
    "SystemDrive",
    "windir",
    "ComSpec",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
    "OS",
    "LANG",
];

/// Parent variables copied by default. None of them carries credentials.
#[cfg(not(windows))]
pub const DEFAULT_ENV_ALLOWLIST: &[&str] = &["PATH", "LANG", "LC_ALL", "LC_CTYPE", "TZ"];

/// Variables the sandbox always sets itself; they can be neither inherited nor overridden.
#[cfg(windows)]
pub const MANAGED_ENV_VARS: &[&str] = &[
    "TEMP",
    "TMP",
    "TMPDIR",
    "HOME",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
];

/// Variables the sandbox always sets itself; they can be neither inherited nor overridden.
#[cfg(not(windows))]
pub const MANAGED_ENV_VARS: &[&str] = &["TEMP", "TMP", "TMPDIR", "HOME"];

/// Name fragments that mark a variable as likely to hold a credential.
const SECRET_FRAGMENTS: &[&str] = &[
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "PRIVATE",
    "APIKEY",
    "API_KEY",
    "AUTH",
    "COOKIE",
    "SESSION",
];

/// Prefixes of provider credential families that are never inherited implicitly.
const SECRET_PREFIXES: &[&str] = &["AWS_", "AZURE_", "GOOGLE_", "GCP_", "SSH_", "GPG_"];

/// Whether an environment variable name looks like it carries a credential. Such names
/// may not be inherited through the allowlist; the broker passes resolved secrets
/// explicitly (`SandboxRequest::env`) for the specific tool that needs them.
pub fn is_secret_like(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper == "KEY"
        || upper.ends_with("_KEY")
        || SECRET_FRAGMENTS
            .iter()
            .any(|fragment| upper.contains(fragment))
        || SECRET_PREFIXES
            .iter()
            .any(|prefix| upper.starts_with(prefix))
}

pub(crate) fn names_equal(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn is_managed(name: &str) -> bool {
    MANAGED_ENV_VARS
        .iter()
        .any(|managed| names_equal(managed, name))
}

fn validate_name(name: &str) -> Result<(), SandboxError> {
    if name.is_empty() || name.contains('=') || name.contains('\0') {
        return Err(SandboxError::invalid(format!(
            "invalid environment variable name {name:?}"
        )));
    }
    Ok(())
}

/// Directories exported to the child as its temporary and home directories.
pub(crate) struct ScratchDirs<'a> {
    pub temp: &'a Path,
    pub home: &'a Path,
}

/// Build the complete child environment from a cleared slate.
pub(crate) fn build_environment(
    allowlist: &[String],
    additions: &[(String, String)],
    parent: impl IntoIterator<Item = (OsString, OsString)>,
    scratch: &ScratchDirs<'_>,
) -> Result<Vec<(OsString, OsString)>, SandboxError> {
    for name in allowlist {
        validate_name(name)?;
        if is_managed(name) {
            return Err(SandboxError::invalid(format!(
                "{name} is managed by the sandbox and cannot be inherited"
            )));
        }
        if is_secret_like(name) {
            return Err(SandboxError::invalid(format!(
                "{name} looks like a credential and cannot be inherited; pass a resolved secret through explicit env"
            )));
        }
    }
    for (name, value) in additions {
        validate_name(name)?;
        if is_managed(name) {
            return Err(SandboxError::invalid(format!(
                "{name} is managed by the sandbox and cannot be overridden"
            )));
        }
        if value.contains('\0') {
            return Err(SandboxError::invalid(format!(
                "value of {name} contains a NUL byte"
            )));
        }
    }

    let mut result: Vec<(OsString, OsString)> = Vec::new();
    for (name, value) in parent {
        let Some(text) = name.to_str() else { continue };
        if allowlist.iter().any(|allowed| names_equal(allowed, text))
            && !result
                .iter()
                .any(|(existing, _)| existing.to_str().is_some_and(|e| names_equal(e, text)))
        {
            result.push((name, value));
        }
    }

    let temp = scratch.temp.as_os_str().to_owned();
    let home = scratch.home.as_os_str().to_owned();
    for name in ["TEMP", "TMP", "TMPDIR"] {
        result.push((name.into(), temp.clone()));
    }
    result.push(("HOME".into(), home.clone()));
    #[cfg(windows)]
    {
        result.push(("USERPROFILE".into(), home.clone()));
        result.push((
            "APPDATA".into(),
            scratch
                .home
                .join("AppData")
                .join("Roaming")
                .into_os_string(),
        ));
        result.push((
            "LOCALAPPDATA".into(),
            scratch.home.join("AppData").join("Local").into_os_string(),
        ));
    }

    for (name, value) in additions {
        result.retain(|(existing, _)| !existing.to_str().is_some_and(|e| names_equal(e, name)));
        result.push((name.into(), value.into()));
    }
    Ok(result)
}

/// Read the parent's environment.
pub(crate) fn parent_environment() -> Vec<(OsString, OsString)> {
    env::vars_os().collect()
}

/// Look up `name` in a built environment.
pub(crate) fn lookup<'a>(environment: &'a [(OsString, OsString)], name: &str) -> Option<&'a OsStr> {
    environment
        .iter()
        .find(|(candidate, _)| candidate.to_str().is_some_and(|c| names_equal(c, name)))
        .map(|(_, value)| value.as_os_str())
}

#[cfg(windows)]
const EXECUTABLE_EXTENSIONS: &[&str] = &["com", "exe", "bat", "cmd"];

/// Resolve `program` to an absolute, canonical executable path.
///
/// Absolute paths must exist. Paths with separators resolve against `cwd`. Bare names
/// are searched only in the absolute directories of `search_path` (never the current
/// directory); on Windows a bare name without an extension also tries `.com`, `.exe`,
/// `.bat`, and `.cmd`.
pub fn resolve_program(
    program: &str,
    search_path: Option<&OsStr>,
    cwd: &Path,
) -> Result<PathBuf, SandboxError> {
    if program.is_empty() || program.contains('\0') {
        return Err(SandboxError::ProgramNotFound(
            "program name must be non-empty".into(),
        ));
    }
    let candidate = Path::new(program);
    if candidate.is_absolute() {
        return paths::canonical_file(candidate);
    }
    if candidate.components().count() > 1 || program.contains('/') || program.contains('\\') {
        return paths::canonical_file(&cwd.join(candidate));
    }
    let Some(search_path) = search_path else {
        return Err(SandboxError::ProgramNotFound(format!(
            "{program}: no PATH in the sandbox environment"
        )));
    };
    for directory in env::split_paths(search_path) {
        if !directory.is_absolute() {
            continue;
        }
        for name in candidate_names(program) {
            let path = directory.join(&name);
            if path.is_file() {
                return paths::canonical_file(&path);
            }
        }
    }
    Err(SandboxError::ProgramNotFound(format!(
        "{program} was not found in the sandbox PATH"
    )))
}

fn candidate_names(program: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        if Path::new(program).extension().is_some() {
            vec![program.to_owned()]
        } else {
            EXECUTABLE_EXTENSIONS
                .iter()
                .map(|extension| format!("{program}.{extension}"))
                .collect()
        }
    }
    #[cfg(not(windows))]
    {
        vec![program.to_owned()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let temp = dir.path().join("tmp");
        let home = dir.path().join("home");
        (dir, temp, home)
    }

    #[test]
    fn secret_like_names_are_detected() {
        for name in [
            "AIDEX_TEST_SECRET_TOKEN",
            "GITHUB_TOKEN",
            "OPENAI_API_KEY",
            "AWS_ACCESS_KEY_ID",
            "SSH_AUTH_SOCK",
            "DB_PASSWORD",
            "my_key",
        ] {
            assert!(is_secret_like(name), "{name}");
        }
        for name in ["PATH", "LANG", "SystemRoot", "KEYBOARD_LAYOUT", "TZ"] {
            assert!(!is_secret_like(name), "{name}");
        }
    }

    #[test]
    fn only_allowlisted_parent_variables_are_copied() {
        let (_dir, temp, home) = scratch();
        let parent = vec![
            (OsString::from("PATH"), OsString::from("/bin")),
            (
                OsString::from("AIDEX_TEST_SECRET_TOKEN"),
                OsString::from("hunter2"),
            ),
            (OsString::from("UNLISTED"), OsString::from("x")),
        ];
        let environment = build_environment(
            &["PATH".to_owned()],
            &[],
            parent,
            &ScratchDirs {
                temp: &temp,
                home: &home,
            },
        )
        .expect("environment");
        assert_eq!(lookup(&environment, "PATH"), Some(OsStr::new("/bin")));
        assert_eq!(lookup(&environment, "AIDEX_TEST_SECRET_TOKEN"), None);
        assert_eq!(lookup(&environment, "UNLISTED"), None);
        assert_eq!(lookup(&environment, "TMP"), Some(temp.as_os_str()));
        assert_eq!(lookup(&environment, "HOME"), Some(home.as_os_str()));
    }

    #[test]
    fn secret_allowlist_entries_and_managed_overrides_are_rejected() {
        let (_dir, temp, home) = scratch();
        let scratch = ScratchDirs {
            temp: &temp,
            home: &home,
        };
        let error = build_environment(&["GITHUB_TOKEN".to_owned()], &[], Vec::new(), &scratch)
            .expect_err("secret allowlist");
        assert!(matches!(error, SandboxError::InvalidRequest(_)));
        let error = build_environment(
            &[],
            &[("TEMP".into(), "/elsewhere".into())],
            Vec::new(),
            &scratch,
        )
        .expect_err("managed override");
        assert!(matches!(error, SandboxError::InvalidRequest(_)));
        let error = build_environment(&["BAD=NAME".to_owned()], &[], Vec::new(), &scratch)
            .expect_err("bad name");
        assert!(matches!(error, SandboxError::InvalidRequest(_)));
    }

    #[test]
    fn explicit_additions_override_inherited_values() {
        let (_dir, temp, home) = scratch();
        let parent = vec![(OsString::from("LANG"), OsString::from("C"))];
        let environment = build_environment(
            &["LANG".to_owned()],
            &[
                ("LANG".into(), "en_US.UTF-8".into()),
                ("TOOL_TOKEN".into(), "scoped".into()),
            ],
            parent,
            &ScratchDirs {
                temp: &temp,
                home: &home,
            },
        )
        .expect("environment");
        assert_eq!(
            lookup(&environment, "LANG"),
            Some(OsStr::new("en_US.UTF-8"))
        );
        assert_eq!(
            lookup(&environment, "TOOL_TOKEN"),
            Some(OsStr::new("scoped"))
        );
        assert_eq!(
            environment
                .iter()
                .filter(|(name, _)| name.to_str() == Some("LANG"))
                .count(),
            1
        );
    }

    #[test]
    fn resolve_program_searches_absolute_path_entries_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).expect("bin");
        let name = if cfg!(windows) { "tool.exe" } else { "tool" };
        std::fs::write(bin.join(name), b"").expect("tool");
        let search = env::join_paths([PathBuf::from("relative"), bin.clone()]).expect("join");
        let resolved = resolve_program("tool", Some(&search), dir.path()).expect("resolved");
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with(name));
        assert!(matches!(
            resolve_program("missing-tool", Some(&search), dir.path()),
            Err(SandboxError::ProgramNotFound(_))
        ));
        assert!(matches!(
            resolve_program("tool", None, dir.path()),
            Err(SandboxError::ProgramNotFound(_))
        ));
        let relative = format!("bin/{name}");
        assert!(resolve_program(&relative, None, dir.path()).is_ok());
    }
}
