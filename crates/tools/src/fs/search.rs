//! `fs.search`: bounded literal/regex grep over ignore-aware workspace files.

use std::fs::File;
use std::io::Read;

use aidex_policy::Action;
use globset::GlobMatcher;
use regex::bytes::{Regex, RegexBuilder};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{list::compile_glob_for, parse_dir};
use crate::{
    EntryKind, ExecutionPermit, InputError, PlanParts, RiskProfile, Tool, ToolContext,
    ToolDescriptor, ToolError, ToolOutput, ToolPlan,
    builtin::descriptor,
    diff::looks_binary,
    schema::{normalized, parse_input},
    sensitive::looks_sensitive,
    text,
    walk::{self, Flow, WalkOptions},
};

/// Longest search pattern, in bytes.
const MAX_PATTERN_BYTES: usize = 1024;
/// Compiled regex size bound (guards against pathological patterns).
const REGEX_SIZE_LIMIT: usize = 1 << 20;
/// Most context lines on each side of a match.
const MAX_CONTEXT_LINES: u32 = 10;
/// Longest line text returned, in bytes.
const MAX_LINE_BYTES: usize = 512;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchInput {
    /// Text to find (a literal unless `regex` is true).
    pattern: String,
    /// Interpret `pattern` as a regular expression (Rust `regex` syntax).
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    case_insensitive: bool,
    /// Directory to search beneath (default the workspace root).
    #[serde(default)]
    path: Option<String>,
    /// Only search files whose workspace-relative path matches this glob.
    #[serde(default)]
    glob: Option<String>,
    /// Lines of context before and after each match (0-10).
    #[serde(default)]
    context_lines: u32,
    /// Most matches to return (capped by the configured search limit).
    #[serde(default)]
    max_results: Option<u32>,
    /// Also search files matched by `.gitignore`.
    #[serde(default)]
    include_ignored: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchMatch {
    path: String,
    /// 1-based line number.
    line: u64,
    /// The matching line (bounded).
    text: String,
    before: Vec<String>,
    after: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Skipped {
    /// Files containing NUL bytes.
    binary: u64,
    /// Files above the configured search size limit.
    too_large: u64,
    /// Files whose names look credential-bearing (`.env`, keys); read them with
    /// `fs.read`, which policy governs.
    sensitive: u64,
    /// Files that could not be read, and links (never followed).
    unreadable: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchOutput {
    matches: Vec<SearchMatch>,
    files_searched: u64,
    skipped: Skipped,
    /// A result or walk limit was reached.
    truncated: bool,
}

fn compile(input: &SearchInput) -> Result<Regex, ToolError> {
    if input.pattern.is_empty() || input.pattern.len() > MAX_PATTERN_BYTES {
        return Err(ToolError::invalid(
            "pattern",
            InputError::InvalidPattern,
            "the pattern must be 1-1024 bytes",
        ));
    }
    let source = if input.regex {
        input.pattern.clone()
    } else {
        regex::escape(&input.pattern)
    };
    RegexBuilder::new(&source)
        .case_insensitive(input.case_insensitive)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|error| {
            ToolError::invalid("pattern", InputError::InvalidPattern, error.to_string())
        })
}

fn file_glob(input: &SearchInput) -> Result<Option<GlobMatcher>, ToolError> {
    input
        .glob
        .as_deref()
        .map(|glob| compile_glob_for("glob", glob, input.case_insensitive))
        .transpose()
}

fn validate(input: &SearchInput) -> Result<(), ToolError> {
    if input.context_lines > MAX_CONTEXT_LINES {
        return Err(ToolError::invalid(
            "context_lines",
            InputError::OutOfRange,
            "at most 10 context lines",
        ));
    }
    if input.max_results == Some(0) {
        return Err(ToolError::invalid(
            "max_results",
            InputError::OutOfRange,
            "must be at least 1",
        ));
    }
    compile(input)?;
    file_glob(input)?;
    Ok(())
}

fn line_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    text::truncate(&String::from_utf8_lossy(bytes), MAX_LINE_BYTES)
}

pub(crate) struct FsSearch {
    descriptor: ToolDescriptor,
}

impl FsSearch {
    pub(crate) fn new() -> Result<Self, ToolError> {
        Ok(Self {
            descriptor: descriptor::<SearchInput, SearchOutput>(
                "fs.search",
                1,
                "Search workspace text files for a literal or regular expression, line by line, \
                 honouring .gitignore and skipping binary, oversized, and credential-like files.",
                RiskProfile::read_only(Action::FsSearch),
                &["workspace:read"],
            )?,
        })
    }
}

impl Tool for FsSearch {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError> {
        let input: SearchInput = parse_input(input)?;
        validate(&input)?;
        let dir = parse_dir("path", input.path.as_deref())?;
        let facts = super::list::listing_facts(ctx, dir.as_ref())?;
        let summary = format!(
            "search `{}` in {}",
            input.pattern,
            dir.as_ref().map_or(".", |path| path.as_str())
        );
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
        let input: SearchInput = parse_input(plan.input().clone())?;
        let regex = compile(&input)?;
        let glob = file_glob(&input)?;
        let dir = parse_dir("path", input.path.as_deref())?;
        let max = input
            .max_results
            .map_or(ctx.limits.max_search_results, |max| max as usize)
            .min(ctx.limits.max_search_results);
        let context = input.context_lines as usize;
        let options = WalkOptions {
            include_ignored: input.include_ignored,
            max_depth: None,
            max_entries: ctx.limits.max_walk_entries,
        };
        let mut matches = Vec::new();
        let mut skipped = Skipped::default();
        let mut files_searched = 0u64;
        let mut full = false;
        let summary = walk::walk(ctx, dir.as_ref(), options, |entry| {
            if entry.kind == EntryKind::Symlink {
                skipped.unreadable += 1;
                return Ok(Flow::Continue);
            }
            if entry.kind != EntryKind::File
                || glob
                    .as_ref()
                    .is_some_and(|glob| !glob.is_match(entry.path.as_str()))
            {
                return Ok(Flow::Continue);
            }
            if looks_sensitive(entry.path.as_str()) {
                skipped.sensitive += 1;
                return Ok(Flow::Continue);
            }
            if entry.len > ctx.limits.max_search_file_bytes {
                skipped.too_large += 1;
                return Ok(Flow::Continue);
            }
            let mut bytes = Vec::new();
            let read = File::open(&entry.absolute).and_then(|file| {
                file.take(ctx.limits.max_search_file_bytes + 1)
                    .read_to_end(&mut bytes)
            });
            if read.is_err() {
                skipped.unreadable += 1;
                return Ok(Flow::Continue);
            }
            if bytes.len() as u64 > ctx.limits.max_search_file_bytes {
                skipped.too_large += 1;
                return Ok(Flow::Continue);
            }
            if looks_binary(&bytes) {
                skipped.binary += 1;
                return Ok(Flow::Continue);
            }
            files_searched += 1;
            let lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
            let count = if bytes.ends_with(b"\n") {
                lines.len() - 1
            } else {
                lines.len()
            };
            for index in 0..count {
                if !regex.is_match(lines[index]) {
                    continue;
                }
                if matches.len() == max {
                    full = true;
                    return Ok(Flow::Stop);
                }
                let before = lines[index.saturating_sub(context)..index]
                    .iter()
                    .map(|line| line_text(line))
                    .collect();
                let after = lines[index + 1..(index + 1 + context).min(count)]
                    .iter()
                    .map(|line| line_text(line))
                    .collect();
                matches.push(SearchMatch {
                    path: entry.path.as_str().to_owned(),
                    line: index as u64 + 1,
                    text: line_text(lines[index]),
                    before,
                    after,
                });
            }
            Ok(Flow::Continue)
        })?;
        let output = SearchOutput {
            truncated: full || summary.truncated,
            files_searched,
            skipped,
            matches,
        };
        let text = format!(
            "search `{}` found {} matches in {} files{}",
            input.pattern,
            output.matches.len(),
            output.files_searched,
            if output.truncated { " (truncated)" } else { "" }
        );
        ToolOutput::typed(text, &output)
    }
}
