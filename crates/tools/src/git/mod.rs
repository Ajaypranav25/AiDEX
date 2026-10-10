//! Git tools. Every invocation is a fixed argument vector (no shell) run through
//! the context's [`crate::ProcessLauncher`] with the workspace root as cwd,
//! hardening options (`core.fsmonitor=false`, hooks disabled, no pager, no colour,
//! no external diff or textconv drivers), `GIT_TERMINAL_PROMPT=0`, and
//! `GIT_CEILING_DIRECTORIES` set so git never discovers a repository above the
//! workspace root.

mod history;
mod inspect;
mod mutate;

use std::fs;

use aidex_core::WorkspacePath;
use aidex_sandbox::{KillReason, MountAccess};
use aidex_workspace::Access;

pub(crate) use history::{GitDiff, GitLog, GitShow};
pub(crate) use inspect::{GitBranch, GitStatus};
pub(crate) use mutate::{GitCommit, GitPush};

use crate::{
    ExecutionPermit, InputError, LaunchOutcome, LaunchRequest, PathAccess, PlanPath, ToolContext,
    ToolError, ToolPlan,
    fs::{parse_path, plan_path, resolve, stat},
    launcher, text,
};

/// Longest revision expression accepted.
const MAX_REV_BYTES: usize = 256;
/// Most pathspecs in one call.
const MAX_PATHSPECS: usize = 500;
/// Longest stderr excerpt kept in errors.
const MAX_STDERR_BYTES: usize = 2 * 1024;

/// A hooks directory that cannot exist (a device, not a directory).
const NO_HOOKS: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// How a git invocation uses the workspace.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GitRun {
    /// Writes repository state (index, objects, refs).
    pub writes: bool,
    pub network: bool,
}

/// The workspace root must itself be a repository root (`.git` dir or file).
pub(crate) fn require_repository(ctx: &ToolContext) -> Result<PlanPath, ToolError> {
    let root = ctx.root.root();
    if fs::symlink_metadata(root.join(".git")).is_err() {
        return Err(ToolError::NotARepository);
    }
    Ok(plan_path(None, root, PathAccess::List, &stat(root)?, None))
}

/// Validate pathspecs as confined workspace paths (they may name deleted files).
pub(crate) fn pathspecs(
    ctx: &ToolContext,
    raw: &[String],
    access: PathAccess,
) -> Result<(Vec<WorkspacePath>, Vec<PlanPath>), ToolError> {
    if raw.len() > MAX_PATHSPECS {
        return Err(ToolError::invalid(
            "paths",
            InputError::OutOfRange,
            "at most 500 paths",
        ));
    }
    let mut paths = Vec::with_capacity(raw.len());
    let mut facts = Vec::with_capacity(raw.len());
    for (index, value) in raw.iter().enumerate() {
        let field = format!("paths[{index}]");
        let path = parse_path(&field, value)?;
        let absolute =
            resolve(ctx, &field, &path, Access::Create).or_else(|error| match error {
                // A link leaf inside the workspace is a valid pathspec target.
                ToolError::SymlinkLeaf { .. } => resolve(ctx, &field, &path, Access::Read),
                other => Err(other),
            })?;
        facts.push(plan_path(
            Some(&path),
            &absolute,
            access,
            &stat(&absolute)?,
            None,
        ));
        paths.push(path);
    }
    Ok((paths, facts))
}

/// `-- :(literal)<path>...` so pathspecs are never globs or options.
pub(crate) fn pathspec_args(paths: &[WorkspacePath]) -> Vec<String> {
    let mut args = vec!["--".to_owned()];
    args.extend(
        paths
            .iter()
            .map(|path| format!(":(literal){}", path.as_str())),
    );
    args
}

/// Revisions: no leading `-` (never an option), a conservative character set.
pub(crate) fn validate_rev(field: &str, rev: &str, allow_range: bool) -> Result<(), ToolError> {
    let invalid = |message: &str| ToolError::invalid(field, InputError::InvalidValue, message);
    if rev.is_empty() || rev.len() > MAX_REV_BYTES {
        return Err(invalid("a revision must be 1-256 bytes"));
    }
    if rev.starts_with('-') {
        return Err(invalid("a revision must not start with `-`"));
    }
    if !rev
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "._/~^@{}-".contains(ch))
    {
        return Err(invalid(
            "a revision may only use letters, digits, and ._/~^@{}-",
        ));
    }
    if !allow_range && rev.contains("..") {
        return Err(invalid("a single revision is required, not a range"));
    }
    Ok(())
}

/// Branch names per `git check-ref-format` (conservatively).
pub(crate) fn validate_branch(field: &str, name: &str) -> Result<(), ToolError> {
    let valid = !name.is_empty()
        && name.len() <= MAX_REV_BYTES
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name.contains("/.")
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "._/-".contains(ch));
    if valid {
        Ok(())
    } else {
        Err(ToolError::invalid(
            field,
            InputError::InvalidValue,
            "not a valid branch name",
        ))
    }
}

fn base_args() -> Vec<String> {
    [
        "--no-pager",
        "-c",
        "core.fsmonitor=false",
        "-c",
        &format!("core.hooksPath={NO_HOOKS}"),
        "-c",
        "color.ui=false",
        "-c",
        "core.quotePath=false",
        "-c",
        "log.showSignature=false",
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect()
}

/// Run `git <args>` for `plan`; a non-zero exit is [`ToolError::ProcessFailed`].
pub(crate) fn run(
    ctx: &ToolContext,
    plan: &ToolPlan,
    permit: &ExecutionPermit,
    args: Vec<String>,
    options: GitRun,
    stdin: Option<Vec<u8>>,
) -> Result<LaunchOutcome, ToolError> {
    let mut full = base_args();
    full.extend(args);
    let root = ctx.root.display_root();
    let mut request = LaunchRequest::new(ctx, plan, permit, "git", full)?
        .with_workspace_access(if options.writes {
            MountAccess::ReadWrite
        } else {
            MountAccess::ReadOnly
        })
        .with_network(options.network)
        .with_env("GIT_TERMINAL_PROMPT", "0")
        .with_env("GCM_INTERACTIVE", "never")
        .with_env("GIT_PAGER", "cat")
        .with_env("PAGER", "cat")
        .with_env("LC_ALL", "C");
    if !options.writes {
        request = request.with_env("GIT_OPTIONAL_LOCKS", "0");
    }
    if let Some(parent) = root.parent() {
        request = request.with_env("GIT_CEILING_DIRECTORIES", parent.display().to_string());
    }
    if let Some(bytes) = stdin {
        request = request.with_stdin(bytes);
    }
    let outcome = launcher::launch(ctx, &request)?;
    if outcome.status.killed_by == Some(KillReason::WallTimeout) {
        return Err(ToolError::TimedOut {
            program: "git".into(),
            after_ms: u64::try_from(outcome.status.elapsed.as_millis()).unwrap_or(u64::MAX),
        });
    }
    if !outcome.status.success() {
        let stderr = text::process_text(&outcome.output.stderr.bytes);
        let stderr = text::truncate(stderr.trim(), MAX_STDERR_BYTES);
        if stderr.contains("not a git repository") {
            return Err(ToolError::NotARepository);
        }
        return Err(ToolError::ProcessFailed {
            program: "git".into(),
            exit_code: outcome.status.code,
            stderr,
        });
    }
    Ok(outcome)
}

/// Lossy, redacted stdout and whether it was cut by the output bound.
pub(crate) fn stdout(outcome: &LaunchOutcome) -> (String, bool) {
    (
        text::process_text(&outcome.output.stdout.bytes),
        outcome.output.stdout.truncated,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_cannot_be_options_or_odd_syntax() {
        assert!(validate_rev("rev", "HEAD~2", false).is_ok());
        assert!(validate_rev("rev", "main..feature", true).is_ok());
        assert!(validate_rev("rev", "main..feature", false).is_err());
        assert!(validate_rev("rev", "--output=/tmp/x", true).is_err());
        assert!(validate_rev("rev", "HEAD:secret", true).is_err());
        assert!(validate_rev("rev", "a b", true).is_err());
    }

    #[test]
    fn branch_names_follow_ref_rules() {
        for good in ["main", "feature/x-1", "release_2.0"] {
            assert!(validate_branch("branch", good).is_ok(), "{good}");
        }
        for bad in [
            "-f", "a..b", "x.lock", "a//b", "a@{1}", "", "a b", "+main", "a:b",
        ] {
            assert!(validate_branch("branch", bad).is_err(), "{bad}");
        }
    }
}
