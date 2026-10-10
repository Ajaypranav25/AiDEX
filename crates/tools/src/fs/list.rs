//! `fs.list` and `fs.glob`: ignore-aware, confined directory listings.

use aidex_core::WorkspacePath;
use aidex_policy::Action;
use aidex_workspace::Access;
use globset::{GlobBuilder, GlobMatcher};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{parse_dir, plan_path, resolve, stat};
use crate::{
    EntryKind, ExecutionPermit, InputError, PathAccess, PlanFacts, PlanParts, RiskProfile, Tool,
    ToolContext, ToolDescriptor, ToolError, ToolOutput, ToolPlan,
    builtin::descriptor,
    schema::{normalized, parse_input},
    walk::{self, Flow, WalkOptions},
};

/// Longest glob pattern accepted, in bytes.
const MAX_PATTERN_BYTES: usize = 1024;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ListEntry {
    /// Workspace-relative path.
    pub path: String,
    pub kind: EntryKind,
    /// Size in bytes (files only).
    pub size: Option<u64>,
}

impl ListEntry {
    fn from(entry: &walk::WalkEntry) -> Self {
        Self {
            path: entry.path.as_str().to_owned(),
            kind: entry.kind,
            size: (entry.kind == EntryKind::File).then_some(entry.len),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListInput {
    /// Directory to list (workspace-relative; default the workspace root).
    #[serde(default)]
    path: Option<String>,
    /// List the whole subtree instead of direct children.
    #[serde(default)]
    recursive: bool,
    /// Depth limit when recursive (1 = direct children).
    #[serde(default)]
    max_depth: Option<u32>,
    /// Include entries matched by `.gitignore` files.
    #[serde(default)]
    include_ignored: bool,
    /// Most entries to return (capped by the configured walk limit).
    #[serde(default)]
    max_entries: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListOutput {
    /// The listed directory (`""` = workspace root).
    path: String,
    entries: Vec<ListEntry>,
    /// The entry limit was reached.
    truncated: bool,
    /// Entries skipped because their names are not portable workspace paths.
    unsupported_names: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GlobInput {
    /// Glob over workspace-relative paths: `*` stays within a component, `**`
    /// crosses components (for example `src/**/*.rs`).
    pattern: String,
    /// Directory to search beneath (default the workspace root).
    #[serde(default)]
    path: Option<String>,
    /// Include entries matched by `.gitignore` files.
    #[serde(default)]
    include_ignored: bool,
    /// Match case-insensitively.
    #[serde(default)]
    case_insensitive: bool,
    /// Most matches to return (capped by the configured search limit).
    #[serde(default)]
    max_results: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GlobOutput {
    pattern: String,
    matches: Vec<ListEntry>,
    /// A result or walk limit was reached.
    truncated: bool,
}

fn positive(field: &str, value: Option<u32>) -> Result<(), ToolError> {
    match value {
        Some(0) => Err(ToolError::invalid(
            field,
            InputError::OutOfRange,
            "must be at least 1",
        )),
        _ => Ok(()),
    }
}

/// Plan facts for a listing rooted at `dir` (`None` = workspace root).
pub(super) fn listing_facts(
    ctx: &ToolContext,
    dir: Option<&WorkspacePath>,
) -> Result<PlanFacts, ToolError> {
    let absolute = match dir {
        Some(path) => resolve(ctx, "path", path, Access::Read)?,
        None => ctx.root.root().to_path_buf(),
    };
    let target = stat(&absolute)?;
    if target.kind != Some(EntryKind::Directory) {
        return Err(ToolError::WrongKind {
            path: dir
                .map(WorkspacePath::as_str)
                .unwrap_or_default()
                .to_owned(),
            expected: "directory",
            found: target.kind.map_or("missing entry", EntryKind::as_str),
        });
    }
    Ok(PlanFacts {
        paths: vec![plan_path(dir, &absolute, PathAccess::List, &target, None)],
        ..PlanFacts::default()
    })
}

fn shown(dir: Option<&WorkspacePath>) -> &str {
    dir.map_or(".", WorkspacePath::as_str)
}

pub(crate) struct FsList {
    descriptor: ToolDescriptor,
}

impl FsList {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<ListInput, ListOutput>(
                "fs.list",
                1,
                "List a workspace directory (optionally recursively), honouring .gitignore. \
                 Symbolic links are listed but never followed.",
                RiskProfile::read_only(Action::FsRead),
                &["workspace:read"],
            )?,
        })
    }
}

impl Tool for FsList {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: ListInput = parse_input(input)?;
        positive("max_depth", input.max_depth)?;
        positive("max_entries", input.max_entries)?;
        if input.max_depth.is_some() && !input.recursive {
            return Err(ToolError::invalid(
                "max_depth",
                InputError::ConflictingFields,
                "max_depth requires recursive",
            ));
        }
        let dir = parse_dir("path", input.path.as_deref())?;
        let facts = listing_facts(ctx, dir.as_ref())?;
        let summary = format!("list {}", shown(dir.as_ref()));
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
        _permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: ListInput = parse_input(plan.input().clone())?;
        let dir = parse_dir("path", input.path.as_deref())?;
        let limit = input
            .max_entries
            .map_or(ctx.limits.max_walk_entries, |max| max as usize)
            .min(ctx.limits.max_walk_entries);
        let options = WalkOptions {
            include_ignored: input.include_ignored,
            max_depth: if input.recursive {
                input.max_depth.map(|depth| depth as usize)
            } else {
                Some(1)
            },
            max_entries: limit,
        };
        let mut entries = Vec::new();
        let summary = walk::walk(ctx, dir.as_ref(), options, |entry| {
            entries.push(ListEntry::from(entry));
            Ok(Flow::Continue)
        })?;
        let output = ListOutput {
            path: dir
                .as_ref()
                .map(WorkspacePath::as_str)
                .unwrap_or_default()
                .to_owned(),
            truncated: summary.truncated,
            unsupported_names: summary.unsupported_names as u64,
            entries,
        };
        let text = format!(
            "listed {} ({} entries{})",
            shown(dir.as_ref()),
            output.entries.len(),
            if output.truncated { ", truncated" } else { "" }
        );
        ToolOutput::typed(text, &output)
    }
}

pub(super) fn compile_glob_for(
    field: &str,
    pattern: &str,
    case_insensitive: bool,
) -> Result<GlobMatcher, ToolError> {
    let invalid =
        |message: &str| ToolError::invalid(field, InputError::InvalidPattern, message.to_owned());
    if pattern.is_empty() || pattern.len() > MAX_PATTERN_BYTES {
        return Err(invalid("the pattern must be 1-1024 bytes"));
    }
    if pattern.starts_with('/')
        || pattern.contains('\\')
        || pattern.split('/').any(|part| part == "..")
    {
        return Err(invalid(
            "patterns are workspace-relative: no leading `/`, `\\`, or `..`",
        ));
    }
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .case_insensitive(case_insensitive)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|error| invalid(&error.to_string()))
}

pub(crate) struct FsGlob {
    descriptor: ToolDescriptor,
}

impl FsGlob {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<GlobInput, GlobOutput>(
                "fs.glob",
                1,
                "Find workspace files and directories whose workspace-relative path matches a \
                 glob, honouring .gitignore.",
                RiskProfile::read_only(Action::FsSearch),
                &["workspace:read"],
            )?,
        })
    }
}

impl Tool for FsGlob {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: GlobInput = parse_input(input)?;
        positive("max_results", input.max_results)?;
        compile_glob_for("pattern", &input.pattern, input.case_insensitive)?;
        let dir = parse_dir("path", input.path.as_deref())?;
        let facts = listing_facts(ctx, dir.as_ref())?;
        let summary = format!("glob `{}` in {}", input.pattern, shown(dir.as_ref()));
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
        _permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError> {
        let input: GlobInput = parse_input(plan.input().clone())?;
        let matcher = compile_glob_for("pattern", &input.pattern, input.case_insensitive)?;
        let dir = parse_dir("path", input.path.as_deref())?;
        let max = input
            .max_results
            .map_or(ctx.limits.max_search_results, |max| max as usize)
            .min(ctx.limits.max_search_results);
        let options = WalkOptions {
            include_ignored: input.include_ignored,
            max_depth: None,
            max_entries: ctx.limits.max_walk_entries,
        };
        let mut matches = Vec::new();
        let mut full = false;
        let summary = walk::walk(ctx, dir.as_ref(), options, |entry| {
            if matcher.is_match(entry.path.as_str()) {
                if matches.len() == max {
                    full = true;
                    return Ok(Flow::Stop);
                }
                matches.push(ListEntry::from(entry));
            }
            Ok(Flow::Continue)
        })?;
        let output = GlobOutput {
            pattern: input.pattern.clone(),
            truncated: full || summary.truncated,
            matches,
        };
        let text = format!(
            "glob `{}` matched {} paths{}",
            input.pattern,
            output.matches.len(),
            if output.truncated { " (truncated)" } else { "" }
        );
        ToolOutput::typed(text, &output)
    }
}
