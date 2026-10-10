//! `git.commit` and `git.push`: repository mutations, approval-gated by policy.

use aidex_events::RiskLevel;
use aidex_policy::Action;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{GitRun, pathspec_args, pathspecs, require_repository, run, stdout, validate_branch};
use crate::{
    ExecutionPermit, InputError, NetworkFacts, PathAccess, PlanFacts, PlanParts, RiskProfile, Tool,
    ToolContext, ToolDescriptor, ToolError, ToolOutput, ToolPlan,
    builtin::descriptor,
    schema::{normalized, parse_input},
};

/// Longest commit message, in bytes.
const MAX_MESSAGE_BYTES: usize = 10_000;
/// Lock key that serializes repository mutations.
const REPOSITORY_LOCK: &str = ".git";

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CommitInput {
    /// Commit message (subject line, optional body).
    message: String,
    /// Workspace-relative paths to stage and commit; nothing else is committed,
    /// even if other changes are already staged.
    paths: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CommitOutput {
    oid: String,
    paths: Vec<String>,
}

pub(crate) struct GitCommit {
    descriptor: ToolDescriptor,
}

impl GitCommit {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<CommitInput, CommitOutput>(
                "git.commit",
                1,
                "Stage the given workspace paths and commit exactly those paths with a message. \
                 Repository hooks are not run.",
                RiskProfile {
                    action: Action::GitCommit,
                    base_risk: RiskLevel::Medium,
                    reads_workspace: true,
                    writes_workspace: true,
                    spawns_process: true,
                    network: false,
                    destructive: false,
                    remote_write: false,
                },
                &["workspace:read", "repository:write", "process:git"],
            )?,
        })
    }
}

fn validate_commit(input: &CommitInput) -> Result<(), ToolError> {
    if input.message.trim().is_empty() || input.message.len() > MAX_MESSAGE_BYTES {
        return Err(ToolError::invalid(
            "message",
            InputError::InvalidValue,
            "the message must be non-blank and at most 10000 bytes",
        ));
    }
    if input.paths.is_empty() {
        return Err(ToolError::invalid(
            "paths",
            InputError::OutOfRange,
            "name at least one path to commit",
        ));
    }
    Ok(())
}

impl Tool for GitCommit {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: CommitInput = parse_input(input)?;
        validate_commit(&input)?;
        let root = require_repository(ctx)?;
        let (_, mut paths) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        paths.insert(0, root);
        let subject = input.message.lines().next().unwrap_or_default();
        let summary = format!("git commit {} paths: {subject}", input.paths.len());
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
        let input: CommitInput = parse_input(plan.input().clone())?;
        validate_commit(&input)?;
        let (paths, _) = pathspecs(ctx, &input.paths, PathAccess::Read)?;
        let writes = GitRun {
            writes: true,
            network: false,
        };
        let mut add = vec!["add".to_owned(), "--all".to_owned()];
        add.extend(pathspec_args(&paths));
        run(ctx, plan, permit, add, writes, None)?;
        let mut commit: Vec<String> = [
            "commit",
            "--no-verify",
            "--no-edit",
            "--cleanup=whitespace",
            "--file=-",
            "--only",
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();
        commit.extend(pathspec_args(&paths));
        run(
            ctx,
            plan,
            permit,
            commit,
            writes,
            Some(input.message.clone().into_bytes()),
        )?;
        let head = run(
            ctx,
            plan,
            permit,
            vec![
                "rev-parse".to_owned(),
                "--verify".to_owned(),
                "HEAD".to_owned(),
            ],
            GitRun::default(),
            None,
        )?;
        let oid = stdout(&head).0.trim().to_owned();
        let summary = format!("committed {} paths as {oid}", paths.len());
        ToolOutput::typed(
            summary,
            &CommitOutput {
                oid,
                paths: paths.iter().map(|path| path.as_str().to_owned()).collect(),
            },
        )
    }

    fn write_locks(&self, _plan: &ToolPlan) -> Vec<String> {
        vec![REPOSITORY_LOCK.to_owned()]
    }
}

fn default_remote() -> String {
    "origin".to_owned()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PushInput {
    /// Configured remote name (default `origin`); URLs are not accepted.
    #[serde(default = "default_remote")]
    remote: String,
    /// Local branch to push to the same name on the remote. Force pushes are not
    /// supported.
    branch: String,
    /// Record the remote branch as the upstream.
    #[serde(default)]
    set_upstream: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PushOutput {
    remote: String,
    branch: String,
    /// `git push --porcelain` status lines.
    report: Vec<String>,
}

fn validate_push(input: &PushInput) -> Result<(), ToolError> {
    let remote_ok = !input.remote.is_empty()
        && input.remote.len() <= 128
        && !input.remote.starts_with(['-', '.'])
        && input
            .remote
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "._-".contains(ch));
    if !remote_ok {
        return Err(ToolError::invalid(
            "remote",
            InputError::InvalidValue,
            "a configured remote name is required (letters, digits, ._-)",
        ));
    }
    validate_branch("branch", &input.branch)
}

pub(crate) struct GitPush {
    descriptor: ToolDescriptor,
}

impl GitPush {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<PushInput, PushOutput>(
                "git.push",
                1,
                "Push one local branch to the same branch on a configured remote (no force). \
                 Uploads commits to a remote system; requires approval under every profile.",
                RiskProfile {
                    action: Action::GitPush,
                    base_risk: RiskLevel::High,
                    reads_workspace: true,
                    writes_workspace: false,
                    spawns_process: true,
                    network: true,
                    destructive: false,
                    remote_write: true,
                },
                &["repository:read", "network:remote", "process:git"],
            )?,
        })
    }
}

impl Tool for GitPush {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: PushInput = parse_input(input)?;
        validate_push(&input)?;
        let root = require_repository(ctx)?;
        let summary = format!("git push {} {}", input.remote, input.branch);
        let facts = PlanFacts {
            paths: vec![root],
            network: Some(NetworkFacts {
                targets: Vec::new(),
                unresolved: vec![format!("git remote `{}`", input.remote)],
            }),
            remote_write: true,
            ..PlanFacts::default()
        };
        ToolPlan::new(
            ctx,
            &self.descriptor,
            PlanParts::new(normalized(&input)?, summary).with_facts(facts),
        )
    }

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: PushInput = parse_input(plan.input().clone())?;
        validate_push(&input)?;
        let mut args = vec!["push".to_owned(), "--porcelain".to_owned()];
        if input.set_upstream {
            args.push("--set-upstream".to_owned());
        }
        args.push("--end-of-options".to_owned());
        args.push(input.remote.clone());
        args.push(format!("refs/heads/{0}:refs/heads/{0}", input.branch));
        let outcome = run(
            ctx,
            plan,
            permit,
            args,
            GitRun {
                writes: true,
                network: true,
            },
            None,
        )?;
        let report: Vec<String> = stdout(&outcome)
            .0
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect();
        let summary = format!("pushed {} to {}", input.branch, input.remote);
        ToolOutput::typed(
            summary,
            &PushOutput {
                remote: input.remote,
                branch: input.branch,
                report,
            },
        )
    }

    fn write_locks(&self, _plan: &ToolPlan) -> Vec<String> {
        vec![REPOSITORY_LOCK.to_owned()]
    }
}
