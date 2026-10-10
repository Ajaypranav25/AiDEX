//! `git.status` and `git.branch`: read-only repository state.

use aidex_policy::Action;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GitRun, pathspec_args, pathspecs, require_repository, run, stdout};
use crate::{
    ExecutionPermit, PathAccess, PlanFacts, PlanParts, RiskProfile, Tool, ToolContext,
    ToolDescriptor, ToolError, ToolOutput, ToolPlan,
    builtin::descriptor,
    schema::{normalized, parse_input},
};

fn git_read_risk() -> RiskProfile {
    RiskProfile {
        spawns_process: true,
        ..RiskProfile::read_only(Action::GitRead)
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StatusInput {
    /// Limit to these workspace-relative paths (literal pathspecs).
    #[serde(default)]
    paths: Vec<String>,
    /// Report untracked files (default true).
    #[serde(default = "yes")]
    untracked: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BranchState {
    /// Current commit (`(initial)` before the first commit).
    oid: Option<String>,
    /// Current branch (`(detached)` when HEAD is detached).
    head: Option<String>,
    upstream: Option<String>,
    ahead: Option<u64>,
    behind: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum StatusKind {
    Ordinary,
    Renamed,
    Copied,
    Unmerged,
    Untracked,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StatusEntry {
    /// Path relative to the repository (= workspace) root.
    path: String,
    /// Source path of a rename or copy.
    orig_path: Option<String>,
    kind: StatusKind,
    /// Index (staged) status letter (`.` = unchanged).
    index: String,
    /// Worktree status letter (`.` = unchanged).
    worktree: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StatusOutput {
    branch: BranchState,
    entries: Vec<StatusEntry>,
    /// Output exceeded the configured bound; later entries are missing.
    truncated: bool,
}

/// Parse `git status --porcelain=v2 --branch -z`.
fn parse_status(raw: &str, truncated: bool) -> StatusOutput {
    let mut records: Vec<&str> = raw.split('\0').collect();
    if truncated || !raw.ends_with('\0') {
        // The final record may be cut short.
        records.pop();
    }
    let mut branch = BranchState::default();
    let mut entries = Vec::new();
    let mut iter = records.into_iter().filter(|record| !record.is_empty());
    while let Some(record) = iter.next() {
        if let Some(header) = record.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" => branch.oid = Some(value.to_owned()),
                "branch.head" => branch.head = Some(value.to_owned()),
                "branch.upstream" => branch.upstream = Some(value.to_owned()),
                "branch.ab" => {
                    let mut counts = value.split(' ');
                    branch.ahead = counts
                        .next()
                        .and_then(|ahead| ahead.trim_start_matches('+').parse().ok());
                    branch.behind = counts
                        .next()
                        .and_then(|behind| behind.trim_start_matches('-').parse().ok());
                }
                _ => {}
            }
            continue;
        }
        let letters = |xy: &str| {
            let mut chars = xy.chars();
            (
                chars.next().unwrap_or('.').to_string(),
                chars.next().unwrap_or('.').to_string(),
            )
        };
        let (kind, fields) = match record.as_bytes().first() {
            Some(b'1') => (StatusKind::Ordinary, 9),
            Some(b'2') => (StatusKind::Renamed, 10),
            Some(b'u') => (StatusKind::Unmerged, 11),
            Some(b'?') => (StatusKind::Untracked, 2),
            _ => continue,
        };
        let parts: Vec<&str> = record.splitn(fields, ' ').collect();
        let Some(path) = parts.last().filter(|_| parts.len() == fields) else {
            continue;
        };
        let (index, worktree) = match kind {
            StatusKind::Untracked => ("?".to_owned(), "?".to_owned()),
            _ => letters(parts.get(1).copied().unwrap_or("..")),
        };
        let mut entry = StatusEntry {
            path: (*path).to_owned(),
            orig_path: None,
            kind,
            index,
            worktree,
        };
        if matches!(entry.kind, StatusKind::Renamed) {
            if parts.get(8).is_some_and(|score| score.starts_with('C')) {
                entry.kind = StatusKind::Copied;
            }
            entry.orig_path = iter.next().map(str::to_owned);
        }
        entries.push(entry);
    }
    StatusOutput {
        branch,
        entries,
        truncated,
    }
}

pub(crate) struct GitStatus {
    descriptor: ToolDescriptor,
}

impl GitStatus {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<StatusInput, StatusOutput>(
                "git.status",
                1,
                "Show the branch and the staged, unstaged, and untracked changes of the \
                 workspace repository (git status porcelain v2).",
                git_read_risk(),
                &["workspace:read", "process:git"],
            )?,
        })
    }
}

impl Tool for GitStatus {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: StatusInput = parse_input(input)?;
        let root = require_repository(ctx)?;
        let (_, mut paths) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        paths.insert(0, root);
        let facts = PlanFacts {
            paths,
            ..PlanFacts::default()
        };
        ToolPlan::new(
            ctx,
            &self.descriptor,
            PlanParts::new(normalized(&input)?, "git status").with_facts(facts),
        )
    }

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: StatusInput = parse_input(plan.input().clone())?;
        let (paths, _) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        let mut args: Vec<String> = [
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            if input.untracked {
                "--untracked-files=normal"
            } else {
                "--untracked-files=no"
            },
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        args.extend(pathspec_args(&paths));
        let outcome = run(ctx, plan, permit, args, GitRun::default(), None)?;
        let (raw, truncated) = stdout(&outcome);
        let output = parse_status(&raw, truncated);
        let summary = format!(
            "git status: {} changed entries on {}",
            output.entries.len(),
            output.branch.head.as_deref().unwrap_or("unknown branch")
        );
        ToolOutput::typed(summary, &output)
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BranchInput {
    /// Include remote-tracking branches.
    #[serde(default)]
    all: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BranchEntry {
    name: String,
    oid: String,
    upstream: Option<String>,
    current: bool,
    remote: bool,
    ahead: Option<u64>,
    behind: Option<u64>,
    /// The upstream branch no longer exists.
    upstream_gone: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BranchOutput {
    current: Option<String>,
    branches: Vec<BranchEntry>,
    truncated: bool,
}

fn parse_track(track: &str) -> (Option<u64>, Option<u64>, bool) {
    let mut ahead = None;
    let mut behind = None;
    for part in track.split(',').map(str::trim) {
        if let Some(count) = part.strip_prefix("ahead ") {
            ahead = count.parse().ok();
        } else if let Some(count) = part.strip_prefix("behind ") {
            behind = count.parse().ok();
        }
    }
    (ahead, behind, track.trim() == "gone")
}

fn parse_branches(raw: &str, truncated: bool) -> BranchOutput {
    let mut lines: Vec<&str> = raw.split('\n').collect();
    if truncated || !raw.ends_with('\n') {
        lines.pop();
    }
    let mut branches = Vec::new();
    for line in lines.into_iter().filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split('\u{1f}').collect();
        let [refname, oid, upstream, head, track] = fields[..] else {
            continue;
        };
        let (name, remote) = if let Some(name) = refname.strip_prefix("refs/heads/") {
            (name, false)
        } else if let Some(name) = refname.strip_prefix("refs/remotes/") {
            if name.ends_with("/HEAD") {
                continue;
            }
            (name, true)
        } else {
            continue;
        };
        let (ahead, behind, upstream_gone) = parse_track(track);
        branches.push(BranchEntry {
            name: name.to_owned(),
            oid: oid.to_owned(),
            upstream: (!upstream.is_empty()).then(|| upstream.to_owned()),
            current: head == "*",
            remote,
            ahead,
            behind,
            upstream_gone,
        });
    }
    BranchOutput {
        current: branches
            .iter()
            .find(|branch| branch.current)
            .map(|branch| branch.name.clone()),
        branches,
        truncated,
    }
}

pub(crate) struct GitBranch {
    descriptor: ToolDescriptor,
}

impl GitBranch {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<BranchInput, BranchOutput>(
                "git.branch",
                1,
                "List the workspace repository's branches with their upstreams and \
                 ahead/behind counts (read-only).",
                git_read_risk(),
                &["workspace:read", "process:git"],
            )?,
        })
    }
}

impl Tool for GitBranch {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: BranchInput = parse_input(input)?;
        let facts = PlanFacts {
            paths: vec![require_repository(ctx)?],
            ..PlanFacts::default()
        };
        ToolPlan::new(
            ctx,
            &self.descriptor,
            PlanParts::new(normalized(&input)?, "git branch --list").with_facts(facts),
        )
    }

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: BranchInput = parse_input(plan.input().clone())?;
        let mut args = vec![
            "for-each-ref".to_owned(),
            "--format=%(refname)%1f%(objectname)%1f%(upstream:short)%1f%(HEAD)%1f%(upstream:track,nobracket)"
                .to_owned(),
            "refs/heads".to_owned(),
        ];
        if input.all {
            args.push("refs/remotes".to_owned());
        }
        let outcome = run(ctx, plan, permit, args, GitRun::default(), None)?;
        let (raw, truncated) = stdout(&outcome);
        let output = parse_branches(&raw, truncated);
        let summary = format!(
            "git branch: {} branches, current {}",
            output.branches.len(),
            output.current.as_deref().unwrap_or("none")
        );
        ToolOutput::typed(summary, &output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2_records() {
        let raw = "# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 h1 h2 src/a b.rs\0\
2 R. N... 100644 100644 100644 h1 h2 R100 new.rs\0old.rs\0\
? notes.txt\0";
        let status = parse_status(raw, false);
        assert_eq!(status.branch.head.as_deref(), Some("main"));
        assert_eq!(
            (status.branch.ahead, status.branch.behind),
            (Some(2), Some(1))
        );
        assert_eq!(status.entries.len(), 3);
        assert_eq!(status.entries[0].path, "src/a b.rs");
        assert_eq!(status.entries[0].worktree, "M");
        assert_eq!(status.entries[1].orig_path.as_deref(), Some("old.rs"));
        assert!(matches!(status.entries[2].kind, StatusKind::Untracked));
    }

    #[test]
    fn truncated_output_drops_the_partial_record() {
        let status = parse_status("? a.txt\0? b.t", true);
        assert_eq!(status.entries.len(), 1);
        assert!(status.truncated);
    }

    #[test]
    fn parses_branch_listing() {
        let raw = "refs/heads/main\u{1f}abc\u{1f}origin/main\u{1f}*\u{1f}ahead 1, behind 2\n\
refs/remotes/origin/HEAD\u{1f}abc\u{1f}\u{1f} \u{1f}\n\
refs/remotes/origin/main\u{1f}def\u{1f}\u{1f} \u{1f}\n";
        let output = parse_branches(raw, false);
        assert_eq!(output.current.as_deref(), Some("main"));
        assert_eq!(output.branches.len(), 2);
        assert_eq!(output.branches[0].ahead, Some(1));
        assert_eq!(output.branches[0].behind, Some(2));
        assert!(output.branches[1].remote);
    }
}
