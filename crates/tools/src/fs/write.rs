//! `fs.write`: create or replace a file with a reviewable diff and atomic commit.

use aidex_events::{FileChange, RiskLevel};
use aidex_policy::Action;
use aidex_workspace::Access;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    atomic_write, ensure_parents, implied_actions, parse_path, plan_path, read_capped,
    reject_git_internal, resolve, sha256_hex, stat, verify_unchanged,
};
use crate::{
    DiffPreview, EntryKind, ExecutionPermit, PathAccess, PathChange, PlanFacts, PlanParts,
    RiskProfile, Tool, ToolContext, ToolDescriptor, ToolError, ToolOutput, ToolPlan,
    builtin::descriptor,
    diff::file_diff,
    schema::{normalized, parse_input},
};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteInput {
    /// Workspace-relative path of the file to create or replace.
    path: String,
    /// The complete new content (UTF-8 text).
    content: String,
    /// Fail if the file already exists.
    #[serde(default)]
    create_only: bool,
    /// Only replace the file if its current SHA-256 (from `fs.read`) matches.
    #[serde(default)]
    expected_sha256: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteOutput {
    path: String,
    /// The file did not exist before.
    created: bool,
    /// The content differed from what was there (false: nothing was written).
    changed: bool,
    bytes: u64,
    sha256: String,
    additions: u64,
    deletions: u64,
}

pub(crate) struct FsWrite {
    descriptor: ToolDescriptor,
}

impl FsWrite {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<WriteInput, WriteOutput>(
                "fs.write",
                1,
                "Create or overwrite a workspace text file with the given content. Returns a \
                 diff for review before it is applied; missing parent directories are created \
                 inside the workspace.",
                RiskProfile {
                    action: Action::FsWrite,
                    base_risk: RiskLevel::Medium,
                    reads_workspace: true,
                    writes_workspace: true,
                    spawns_process: false,
                    network: false,
                    destructive: false,
                    remote_write: false,
                },
                &["workspace:read", "workspace:write"],
            )?,
        })
    }
}

/// Check the guards that depend on the current file; returns its content.
fn current_content(
    ctx: &ToolContext,
    input: &WriteInput,
    display: &str,
    absolute: &std::path::Path,
    kind: Option<EntryKind>,
) -> Result<Option<Vec<u8>>, ToolError> {
    match kind {
        None => {
            if input.expected_sha256.is_some() {
                return Err(ToolError::conflict(
                    display,
                    "expected_sha256 was given but the file does not exist",
                ));
            }
            Ok(None)
        }
        Some(EntryKind::File) => {
            if input.create_only {
                return Err(ToolError::conflict(display, "the file already exists"));
            }
            let bytes = read_capped(absolute, ctx.limits.max_write_bytes, "the existing file")?;
            if let Some(expected) = &input.expected_sha256
                && !expected.eq_ignore_ascii_case(&sha256_hex(&bytes))
            {
                return Err(ToolError::conflict(
                    display,
                    "its SHA-256 differs from expected_sha256",
                ));
            }
            Ok(Some(bytes))
        }
        Some(other) => Err(ToolError::WrongKind {
            path: display.to_owned(),
            expected: "file",
            found: other.as_str(),
        }),
    }
}

impl Tool for FsWrite {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: WriteInput = parse_input(input)?;
        let path = parse_path("path", &input.path)?;
        reject_git_internal(&path)?;
        let size = input.content.len() as u64;
        if size > ctx.limits.max_write_bytes {
            return Err(ToolError::TooLarge {
                what: "the new content".into(),
                size,
                limit: ctx.limits.max_write_bytes,
            });
        }
        let absolute = resolve(ctx, "path", &path, Access::Create)?;
        let target = stat(&absolute)?;
        let before = current_content(ctx, &input, path.as_str(), &absolute, target.kind)?;
        let mut budget = ctx.limits.max_diff_bytes;
        let diff = file_diff(
            &path,
            before.as_deref(),
            Some(input.content.as_bytes()),
            &mut budget,
        );
        let access = if target.exists() {
            PathAccess::Write
        } else {
            PathAccess::Create
        };
        let facts = PlanFacts {
            paths: vec![plan_path(
                Some(&path),
                &absolute,
                access,
                &target,
                before.as_deref().map(sha256_hex),
            )],
            implied_actions: implied_actions(&path),
            ..PlanFacts::default()
        };
        let summary = match target.exists() {
            false => format!("create {} (+{})", path.as_str(), diff.additions),
            true => format!(
                "overwrite {} (+{} -{})",
                path.as_str(),
                diff.additions,
                diff.deletions
            ),
        };
        ToolPlan::new(
            ctx,
            &self.descriptor,
            PlanParts::new(normalized(&input)?, summary)
                .with_facts(facts)
                .with_preview(DiffPreview::single(diff)),
        )
    }

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        _permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: WriteInput = parse_input(plan.input().clone())?;
        let path = parse_path("path", &input.path)?;
        let planned = plan
            .facts()
            .paths
            .first()
            .ok_or_else(|| ToolError::PlanMismatch("the plan names no path".into()))?;
        let absolute = resolve(ctx, "path", &path, Access::Create)?;
        let current = stat(&absolute)?;
        let bytes = match current.kind {
            Some(EntryKind::File) => Some(read_capped(
                &absolute,
                ctx.limits.max_write_bytes,
                "the existing file",
            )?),
            _ => None,
        };
        verify_unchanged(&path, planned, &current, bytes.as_deref())?;
        let content = input.content.as_bytes();
        let created = !current.exists();
        let changed = bytes.as_deref() != Some(content);
        if changed {
            let destination = ensure_parents(ctx, &path)?;
            atomic_write(&destination, content)?;
        }
        let diff = plan.preview().cloned().unwrap_or_default();
        let file = diff.files.first();
        let output = WriteOutput {
            path: path.as_str().to_owned(),
            created,
            changed,
            bytes: content.len() as u64,
            sha256: sha256_hex(content),
            additions: file.map_or(0, |file| file.additions),
            deletions: file.map_or(0, |file| file.deletions),
        };
        let summary = if !changed {
            format!("{} already had this content", path.as_str())
        } else {
            format!(
                "wrote {} (+{} -{})",
                path.as_str(),
                output.additions,
                output.deletions
            )
        };
        let result = ToolOutput::typed(summary, &output)?;
        if !changed {
            return Ok(result);
        }
        let change = if created {
            FileChange::Created
        } else {
            FileChange::Modified
        };
        Ok(result
            .with_diff(diff)
            .with_changes(vec![PathChange { path, change }]))
    }
}
