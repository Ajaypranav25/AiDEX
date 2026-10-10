//! Human versus `--json` output.
//!
//! With `--json`, a successful command prints exactly one compact JSON
//! document on stdout. Failures always go to stderr; with `--json` they are
//! `{"error":{"code":"<name>","exit_code":N,"message":"...","hint":"..."}}` so
//! stdout carries only successful results.

use crate::exit::{CliError, Exit};
use serde::Serialize;
use serde_json::json;
use std::path::Path;

/// A path for messages and copy-pasteable hints, without the Windows
/// verbatim (`\\?\`) prefix that canonicalization adds.
pub fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        text
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Output {
    json: bool,
}

impl Output {
    pub fn new(json: bool) -> Self {
        Self { json }
    }

    pub fn is_json(self) -> bool {
        self.json
    }

    /// Print `value` as JSON, or run `human` to print text.
    pub fn emit<T: Serialize>(self, value: &T, human: impl FnOnce()) -> Result<(), CliError> {
        if self.json {
            let text = serde_json::to_string(value).map_err(|error| {
                CliError::new(Exit::Failure, format!("cannot encode JSON output: {error}"))
            })?;
            println!("{text}");
        } else {
            human();
        }
        Ok(())
    }

    /// Report a failure on stderr.
    pub fn error(self, command: &str, error: &CliError) {
        if self.json {
            eprintln!("{}", error_json(error));
        } else {
            eprintln!("{command}: {}", error.message);
            if let Some(hint) = &error.hint {
                eprintln!("hint: {hint}");
            }
        }
    }
}

pub fn error_json(error: &CliError) -> serde_json::Value {
    let mut body = json!({
        "code": error.exit.name(),
        "exit_code": error.exit.code(),
        "message": error.message,
    });
    if let Some(hint) = &error.hint {
        body["hint"] = json!(hint);
    }
    json!({ "error": body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_errors_carry_stable_code_exit_status_and_hint() {
        let error = CliError::new(Exit::DaemonUnavailable, "daemon is not running")
            .with_hint("run `aidex daemon start`");
        assert_eq!(
            error_json(&error),
            json!({"error": {
                "code": "daemon_unavailable",
                "exit_code": 3,
                "message": "daemon is not running",
                "hint": "run `aidex daemon start`"
            }})
        );
        let plain = CliError::new(Exit::Drive, "no drive");
        assert!(error_json(&plain)["error"].get("hint").is_none());
    }

    #[test]
    fn display_paths_drop_the_windows_verbatim_prefix() {
        assert_eq!(display_path(Path::new(r"\\?\H:\")), r"H:\");
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share")),
            r"\\server\share"
        );
        assert_eq!(display_path(Path::new("/media/aidex")), "/media/aidex");
    }
}
