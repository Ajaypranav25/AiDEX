//! `git.diff`, `git.log`, and `git.show`: bounded, read-only history inspection.

use aidex_policy::Action;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GitRun, pathspec_args, pathspecs, require_repository, run, stdout, validate_rev};
use crate::{
    ExecutionPermit, InputError, PathAccess, PlanFacts, PlanParts, RiskProfile, Tool, ToolContext,
    ToolDescriptor, ToolError, ToolOutput, ToolPlan,
    builtin::descriptor,
    schema::{normalized, parse_input},
};

/// Most context lines around each hunk.
const MAX_CONTEXT: u32 = 20;
/// Default and largest `git.log` page.
const DEFAULT_LOG: u32 = 20;
const MAX_LOG: u32 = 200;
/// Field and record separators used in `--format`.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';
const COMMIT_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%s";

fn git_read_risk() -> RiskProfile {
    RiskProfile {
        spawns_process: true,
        ..RiskProfile::read_only(Action::GitRead)
    }
}

fn default_context() -> u32 {
    3
}

fn check_context(context: u32) -> Result<(), ToolError> {
    if context > MAX_CONTEXT {
        return Err(ToolError::invalid(
            "context_lines",
            InputError::OutOfRange,
            "at most 20 context lines",
        ));
    }
    Ok(())
}

/// Per-file statistics parsed from a unified diff.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DiffFile {
    path: String,
    additions: u64,
    deletions: u64,
    binary: bool,
}

fn diff_files(diff: &str) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut in_hunk = false;
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("diff --git ") {
            in_hunk = false;
            let path = header
                .rsplit_once(" b/")
                .map_or(header, |(_, path)| path)
                .to_owned();
            files.push(DiffFile {
                path,
                additions: 0,
                deletions: 0,
                binary: false,
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if line.starts_with("@@") {
            in_hunk = true;
        } else if !in_hunk {
            if let Some(path) = line.strip_prefix("+++ b/") {
                file.path = path.to_owned();
            } else if line.starts_with("Binary files ") {
                file.binary = true;
            }
        } else if line.starts_with('+') {
            file.additions += 1;
        } else if line.starts_with('-') {
            file.deletions += 1;
        }
    }
    files
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum DiffMode {
    /// Unstaged changes (worktree against the index).
    #[default]
    Worktree,
    /// Staged changes (index against HEAD).
    Staged,
    /// Worktree against `rev` (or `rev` as a range such as `main..feature`).
    Rev,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DiffInput {
    #[serde(default)]
    mode: DiffMode,
    /// Revision or range; required with mode `rev`.
    #[serde(default)]
    rev: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default = "default_context")]
    context_lines: u32,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DiffOutput {
    /// Unified diff text (bounded).
    diff: String,
    files: Vec<DiffFile>,
    truncated: bool,
}

pub(crate) struct GitDiff {
    descriptor: ToolDescriptor,
}

impl GitDiff {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<DiffInput, DiffOutput>(
                "git.diff",
                1,
                "Show the workspace repository's unstaged, staged, or against-revision diff \
                 (no external diff or textconv drivers).",
                git_read_risk(),
                &["workspace:read", "process:git"],
            )?,
        })
    }
}

fn validate_diff(input: &DiffInput) -> Result<(), ToolError> {
    check_context(input.context_lines)?;
    match (input.mode, &input.rev) {
        (DiffMode::Rev, None) => Err(ToolError::invalid(
            "rev",
            InputError::MissingField,
            "mode `rev` requires `rev`",
        )),
        (DiffMode::Rev, Some(rev)) => validate_rev("rev", rev, true),
        (_, Some(_)) => Err(ToolError::invalid(
            "rev",
            InputError::ConflictingFields,
            "`rev` is only used with mode `rev`",
        )),
        (_, None) => Ok(()),
    }
}

impl Tool for GitDiff {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: DiffInput = parse_input(input)?;
        validate_diff(&input)?;
        let root = require_repository(ctx)?;
        let (_, mut paths) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        paths.insert(0, root);
        let summary = match (&input.mode, &input.rev) {
            (DiffMode::Staged, _) => "git diff --cached".to_owned(),
            (DiffMode::Rev, Some(rev)) => format!("git diff {rev}"),
            _ => "git diff".to_owned(),
        };
        ToolPlan::new(
            ctx,
            &self.descriptor,
            PlanParts::new(normalized(&input)?, summary).with_facts(PlanFacts {
                paths,
                ..PlanFacts::default()
            }),
        )
    }

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: DiffInput = parse_input(plan.input().clone())?;
        validate_diff(&input)?;
        let (paths, _) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        let mut args = vec![
            "diff".to_owned(),
            "--no-color".to_owned(),
            "--no-ext-diff".to_owned(),
            "--no-textconv".to_owned(),
            format!("-U{}", input.context_lines),
        ];
        if matches!(input.mode, DiffMode::Staged) {
            args.push("--cached".to_owned());
        }
        args.push("--end-of-options".to_owned());
        if let Some(rev) = &input.rev {
            args.push(rev.clone());
        }
        args.extend(pathspec_args(&paths));
        let outcome = run(ctx, plan, permit, args, GitRun::default(), None)?;
        let (diff, truncated) = stdout(&outcome);
        let output = DiffOutput {
            files: diff_files(&diff),
            diff,
            truncated,
        };
        let summary = format!("git diff: {} files changed", output.files.len());
        ToolOutput::typed(summary, &output)
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Commit {
    oid: String,
    parents: Vec<String>,
    author_name: String,
    author_email: String,
    /// Author date, ISO 8601.
    authored_at: String,
    subject: String,
}

fn parse_commit(fields: &[&str]) -> Option<Commit> {
    let [oid, parents, name, email, date, subject] = fields[..] else {
        return None;
    };
    Some(Commit {
        oid: oid.trim().to_owned(),
        parents: parents.split_whitespace().map(str::to_owned).collect(),
        author_name: name.to_owned(),
        author_email: email.to_owned(),
        authored_at: date.to_owned(),
        subject: subject.to_owned(),
    })
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LogInput {
    /// Revision or range to start from (default HEAD).
    #[serde(default)]
    rev: Option<String>,
    /// Most commits to return (default 20, at most 200).
    #[serde(default)]
    max_count: Option<u32>,
    /// Commits to skip first (paging).
    #[serde(default)]
    skip: u32,
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LogOutput {
    commits: Vec<Commit>,
    truncated: bool,
}

fn validate_log(input: &LogInput) -> Result<(), ToolError> {
    if let Some(rev) = &input.rev {
        validate_rev("rev", rev, true)?;
    }
    match input.max_count {
        Some(0) => Err(ToolError::invalid(
            "max_count",
            InputError::OutOfRange,
            "must be at least 1",
        )),
        Some(count) if count > MAX_LOG => Err(ToolError::invalid(
            "max_count",
            InputError::OutOfRange,
            "at most 200 commits",
        )),
        _ => Ok(()),
    }
}

pub(crate) struct GitLog {
    descriptor: ToolDescriptor,
}

impl GitLog {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<LogInput, LogOutput>(
                "git.log",
                1,
                "List commits of the workspace repository (bounded, newest first).",
                git_read_risk(),
                &["workspace:read", "process:git"],
            )?,
        })
    }
}

impl Tool for GitLog {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: LogInput = parse_input(input)?;
        validate_log(&input)?;
        let root = require_repository(ctx)?;
        let (_, mut paths) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        paths.insert(0, root);
        let summary = format!("git log {}", input.rev.as_deref().unwrap_or("HEAD"));
        ToolPlan::new(
            ctx,
            &self.descriptor,
            PlanParts::new(normalized(&input)?, summary).with_facts(PlanFacts {
                paths,
                ..PlanFacts::default()
            }),
        )
    }

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: LogInput = parse_input(plan.input().clone())?;
        validate_log(&input)?;
        let (paths, _) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        let mut args = vec![
            "log".to_owned(),
            "--no-color".to_owned(),
            format!("--max-count={}", input.max_count.unwrap_or(DEFAULT_LOG)),
            format!("--skip={}", input.skip),
            format!("--format={COMMIT_FORMAT}%x1e"),
            "--end-of-options".to_owned(),
        ];
        if let Some(rev) = &input.rev {
            args.push(rev.clone());
        }
        args.extend(pathspec_args(&paths));
        let outcome = run(ctx, plan, permit, args, GitRun::default(), None)?;
        let (raw, truncated) = stdout(&outcome);
        let mut records: Vec<&str> = raw.split(RECORD).collect();
        // The text after the final separator is empty or a cut-off record.
        records.pop();
        let commits: Vec<Commit> = records
            .iter()
            .filter_map(|record| {
                parse_commit(
                    &record
                        .trim_start_matches('\n')
                        .split(FIELD)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let summary = format!("git log: {} commits", commits.len());
        ToolOutput::typed(summary, &LogOutput { commits, truncated })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ShowInput {
    /// The commit to show (a single revision; tags are peeled).
    rev: String,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default = "default_context")]
    context_lines: u32,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ShowOutput {
    commit: Option<Commit>,
    /// Commit message body (after the subject).
    body: String,
    diff: String,
    files: Vec<DiffFile>,
    truncated: bool,
}

fn validate_show(input: &ShowInput) -> Result<(), ToolError> {
    validate_rev("rev", &input.rev, false)?;
    check_context(input.context_lines)
}

pub(crate) struct GitShow {
    descriptor: ToolDescriptor,
}

impl GitShow {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<ShowInput, ShowOutput>(
                "git.show",
                1,
                "Show one commit of the workspace repository: metadata, message, and diff \
                 (bounded; no external diff or textconv drivers).",
                git_read_risk(),
                &["workspace:read", "process:git"],
            )?,
        })
    }
}

impl Tool for GitShow {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: ShowInput = parse_input(input)?;
        validate_show(&input)?;
        let root = require_repository(ctx)?;
        let (_, mut paths) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        paths.insert(0, root);
        let summary = format!("git show {}", input.rev);
        ToolPlan::new(
            ctx,
            &self.descriptor,
            PlanParts::new(normalized(&input)?, summary).with_facts(PlanFacts {
                paths,
                ..PlanFacts::default()
            }),
        )
    }

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: ShowInput = parse_input(plan.input().clone())?;
        validate_show(&input)?;
        let (paths, _) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        let mut args = vec![
            "show".to_owned(),
            "--no-color".to_owned(),
            "--no-ext-diff".to_owned(),
            "--no-textconv".to_owned(),
            format!("-U{}", input.context_lines),
            format!("--format=%x1e{COMMIT_FORMAT}%x1f%b%x1e"),
            "--end-of-options".to_owned(),
            format!("{}^{{commit}}", input.rev),
        ];
        args.extend(pathspec_args(&paths));
        let outcome = run(ctx, plan, permit, args, GitRun::default(), None)?;
        let (raw, truncated) = stdout(&outcome);
        let mut parts = raw.splitn(3, RECORD);
        let _ = parts.next();
        let header = parts.next().unwrap_or_default();
        let diff = parts.next().unwrap_or_default().trim_start_matches('\n');
        let fields: Vec<&str> = header.split(FIELD).collect();
        let (commit, body) = match fields.split_last() {
            Some((body, commit)) if fields.len() == 7 => {
                (parse_commit(commit), body.trim().to_owned())
            }
            _ => (None, String::new()),
        };
        let output = ShowOutput {
            commit,
            body,
            files: diff_files(diff),
            diff: diff.to_owned(),
            truncated,
        };
        let summary = format!("git show {}: {} files", input.rev, output.files.len());
        ToolOutput::typed(summary, &output)
    }
}

#[cfg(test)]
mod tests {
    use super::diff_files;

    #[test]
    fn counts_lines_per_file() {
        let diff = "diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-old\n+new\n+more\n context\n\
diff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n";
        let files = diff_files(diff);
        assert_eq!(files.len(), 2);
        assert_eq!(
            (
                files[0].path.as_str(),
                files[0].additions,
                files[0].deletions
            ),
            ("a.txt", 2, 1)
        );
        assert!(files[1].binary);
    }
}
