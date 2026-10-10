//! `fs.patch`: exact search/replace edits or a unified diff, previewed and
//! re-applied atomically at execute time.

use aidex_events::{FileChange, RiskLevel};
use aidex_policy::Action;
use aidex_workspace::Access;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    atomic_write, implied_actions, parse_path, plan_path, read_capped, reject_git_internal,
    resolve, sha256_hex, stat, unified, verify_unchanged,
};
use crate::{
    DiffPreview, EntryKind, ExecutionPermit, InputError, PathAccess, PathChange, PlanFacts,
    PlanParts, RiskProfile, Tool, ToolContext, ToolDescriptor, ToolError, ToolOutput, ToolPlan,
    builtin::descriptor,
    diff::{file_diff, looks_binary},
    schema::{normalized, parse_input},
};

/// Most edits in one call.
const MAX_EDITS: usize = 100;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Edit {
    /// Exact text to replace (non-empty, matched byte for byte).
    old: String,
    /// Replacement text.
    new: String,
    /// How many times `old` must occur (default 1); every occurrence is replaced.
    #[serde(default)]
    expected_occurrences: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PatchInput {
    /// Workspace-relative path of an existing text file.
    path: String,
    /// Search/replace edits applied in order. Exactly one of `edits` and
    /// `unified_diff` is required.
    #[serde(default)]
    edits: Option<Vec<Edit>>,
    /// A single-file unified diff (`@@` hunks; file headers are ignored).
    #[serde(default)]
    unified_diff: Option<String>,
    /// Only patch if the file's current SHA-256 (from `fs.read`) matches.
    #[serde(default)]
    expected_sha256: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PatchOutput {
    path: String,
    bytes: u64,
    sha256: String,
    additions: u64,
    deletions: u64,
}

fn validate(input: &PatchInput) -> Result<(), ToolError> {
    match (&input.edits, &input.unified_diff) {
        (Some(_), Some(_)) => Err(ToolError::invalid(
            "unified_diff",
            InputError::ConflictingFields,
            "give either `edits` or `unified_diff`, not both",
        )),
        (None, None) => Err(ToolError::invalid(
            "edits",
            InputError::MissingField,
            "`edits` or `unified_diff` is required",
        )),
        (Some(edits), None) => {
            if edits.is_empty() || edits.len() > MAX_EDITS {
                return Err(ToolError::invalid(
                    "edits",
                    InputError::OutOfRange,
                    "give 1-100 edits",
                ));
            }
            for (index, edit) in edits.iter().enumerate() {
                if edit.old.is_empty() {
                    return Err(ToolError::invalid(
                        format!("edits[{index}].old"),
                        InputError::InvalidValue,
                        "must not be empty",
                    ));
                }
                if edit.expected_occurrences == Some(0) {
                    return Err(ToolError::invalid(
                        format!("edits[{index}].expected_occurrences"),
                        InputError::OutOfRange,
                        "must be at least 1",
                    ));
                }
            }
            Ok(())
        }
        (None, Some(_)) => Ok(()),
    }
}

/// Apply the input's edits or diff to `content`.
fn apply(input: &PatchInput, content: &str) -> Result<String, ToolError> {
    if let Some(diff) = &input.unified_diff {
        return unified::apply(content, diff)
            .map_err(|(index, reason)| ToolError::PatchFailed { index, reason });
    }
    let mut current = content.to_owned();
    for (index, edit) in input.edits.iter().flatten().enumerate() {
        let expected = edit.expected_occurrences.unwrap_or(1) as usize;
        let found = current.matches(edit.old.as_str()).count();
        if found != expected {
            return Err(ToolError::PatchFailed {
                index,
                reason: if found == 0 {
                    "`old` was not found".into()
                } else {
                    format!("`old` occurs {found} times, expected {expected}")
                },
            });
        }
        current = current.replace(edit.old.as_str(), &edit.new);
    }
    Ok(current)
}

/// Read the current file as text.
fn read_text(
    ctx: &ToolContext,
    path: &aidex_core::WorkspacePath,
    absolute: &std::path::Path,
) -> Result<(Vec<u8>, String), ToolError> {
    let bytes = read_capped(absolute, ctx.limits.max_write_bytes, "the file")?;
    if looks_binary(&bytes) {
        return Err(ToolError::invalid(
            "path",
            InputError::InvalidValue,
            format!("`{}` is binary and cannot be patched", path.as_str()),
        ));
    }
    let text = String::from_utf8(bytes.clone()).map_err(|_| {
        ToolError::invalid(
            "path",
            InputError::InvalidValue,
            format!("`{}` is not UTF-8 text", path.as_str()),
        )
    })?;
    Ok((bytes, text))
}

pub(crate) struct FsPatch {
    descriptor: ToolDescriptor,
}

impl FsPatch {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<PatchInput, PatchOutput>(
                "fs.patch",
                1,
                "Modify an existing workspace text file with exact search/replace edits (each \
                 with an expected occurrence count) or a unified diff. Fails without writing if \
                 any edit or hunk does not apply; returns the diff for review.",
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

impl Tool for FsPatch {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: PatchInput = parse_input(input)?;
        validate(&input)?;
        let path = parse_path("path", &input.path)?;
        reject_git_internal(&path)?;
        let absolute = resolve(ctx, "path", &path, Access::Write)?;
        let target = stat(&absolute)?;
        if target.kind != Some(EntryKind::File) {
            return Err(ToolError::WrongKind {
                path: path.as_str().to_owned(),
                expected: "file",
                found: target.kind.map_or("missing entry", EntryKind::as_str),
            });
        }
        let (before, text) = read_text(ctx, &path, &absolute)?;
        let before_sha256 = sha256_hex(&before);
        if let Some(expected) = &input.expected_sha256 {
            if !expected.eq_ignore_ascii_case(&before_sha256) {
                return Err(ToolError::conflict(
                    path.as_str(),
                    "its SHA-256 differs from expected_sha256",
                ));
            }
        }
        let after = apply(&input, &text)?;
        if after.len() as u64 > ctx.limits.max_write_bytes {
            return Err(ToolError::TooLarge {
                what: "the patched file".into(),
                size: after.len() as u64,
                limit: ctx.limits.max_write_bytes,
            });
        }
        let mut budget = ctx.limits.max_diff_bytes;
        let diff = file_diff(&path, Some(&before), Some(after.as_bytes()), &mut budget);
        let summary = format!(
            "patch {} (+{} -{})",
            path.as_str(),
            diff.additions,
            diff.deletions
        );
        let facts = PlanFacts {
            paths: vec![plan_path(
                Some(&path),
                &absolute,
                PathAccess::Write,
                &target,
                Some(before_sha256),
            )],
            implied_actions: implied_actions(&path),
            ..PlanFacts::default()
        };
        ToolPlan::new(
            ctx,
            &self.descriptor,
            PlanParts::new(normalized(&input)?, summary)
                .with_facts(facts)
                .with_preview(DiffPreview::single(diff))
                .with_state(json!({ "after_sha256": sha256_hex(after.as_bytes()) })),
        )
    }

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        _permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: PatchInput = parse_input(plan.input().clone())?;
        let path = parse_path("path", &input.path)?;
        let planned = plan
            .facts()
            .paths
            .first()
            .ok_or_else(|| ToolError::PlanMismatch("the plan names no path".into()))?;
        let absolute = resolve(ctx, "path", &path, Access::Write)?;
        let current = stat(&absolute)?;
        let bytes = match current.kind {
            Some(EntryKind::File) => Some(read_capped(
                &absolute,
                ctx.limits.max_write_bytes,
                "the file",
            )?),
            _ => None,
        };
        verify_unchanged(&path, planned, &current, bytes.as_deref())?;
        let (_, text) = read_text(ctx, &path, &absolute)?;
        let after = apply(&input, &text)?;
        let after_sha256 = sha256_hex(after.as_bytes());
        if plan.state().get("after_sha256").and_then(Value::as_str) != Some(after_sha256.as_str()) {
            return Err(ToolError::PlanMismatch(
                "re-applying the patch did not reproduce the reviewed result".into(),
            ));
        }
        // Re-resolve right before the rename (point-in-time confinement).
        let destination = resolve(ctx, "path", &path, Access::Write)?;
        atomic_write(&destination, after.as_bytes())?;
        let diff = plan.preview().cloned().unwrap_or_default();
        let file = diff.files.first();
        let output = PatchOutput {
            path: path.as_str().to_owned(),
            bytes: after.len() as u64,
            sha256: after_sha256,
            additions: file.map_or(0, |file| file.additions),
            deletions: file.map_or(0, |file| file.deletions),
        };
        let summary = format!(
            "patched {} (+{} -{})",
            path.as_str(),
            output.additions,
            output.deletions
        );
        Ok(ToolOutput::typed(summary, &output)?
            .with_diff(diff)
            .with_changes(vec![PathChange {
                path,
                change: FileChange::Modified,
            }]))
    }
}
