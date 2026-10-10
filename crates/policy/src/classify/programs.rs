//! Per-command classification context and dispatch to the program-family modules.

use super::{
    files, git, interpreters,
    lexer::{Dialect, Segment},
    net, packages,
    shell::{Accumulator, ShellRisk, classify_text},
    system,
};

/// Facts about one simple command that matter to the pipeline around it.
#[derive(Debug, Default)]
pub(crate) struct SegmentFacts {
    /// The command downloads remote content to its output.
    pub fetcher: bool,
    /// The command decodes base64/hex content.
    pub decoder: bool,
    /// The command interprets its standard input as code.
    pub stdin_executor: bool,
    /// Literal text the command prints (`echo`, `printf`), for `echo ... | sh`.
    pub echo_text: Option<String>,
}

/// Classification context for one simple command.
pub(crate) struct Ctx<'a> {
    pub dialect: Dialect,
    pub depth: usize,
    pub acc: &'a mut Accumulator,
    pub facts: &'a mut SegmentFacts,
}

impl Ctx<'_> {
    pub fn raise(&mut self, risk: ShellRisk, reason: impl Into<String>) {
        self.acc.raise(risk, reason);
    }

    /// Classify a command string run by a nested interpreter.
    pub fn nested(&mut self, text: &str, dialect: Dialect) {
        classify_text(text, dialect, self.depth + 1, self.acc);
    }

    /// Classify a string that is evaluated as code, escalating its risk one level.
    pub fn evaluated(&mut self, text: &str, dialect: Dialect, reason: &str) {
        self.acc.evaluator = true;
        let mut inner = Accumulator::default();
        classify_text(text, dialect, self.depth + 1, &mut inner);
        self.acc.merge_escalated(inner, reason);
    }
}

pub(crate) fn classify_segment(
    segment: &Segment,
    dialect: Dialect,
    depth: usize,
    acc: &mut Accumulator,
    facts: &mut SegmentFacts,
) {
    for target in &segment.redirects {
        files::redirect(target, acc);
    }
    net::scan_words(&segment.words, acc, facts);
    let mut ctx = Ctx {
        dialect,
        depth,
        acc,
        facts,
    };
    classify_words(&segment.words, &mut ctx);
}

/// Classify `words` as a program followed by its arguments.
pub(crate) fn classify_words(words: &[String], ctx: &mut Ctx<'_>) {
    let words = skip_assignments(words, ctx.dialect);
    let Some((first, args)) = words.split_first() else {
        return;
    };
    if first.starts_with('$') || (first.starts_with('%') && first.len() > 1) {
        ctx.acc.program("<dynamic>");
        ctx.raise(ShellRisk::High, "program name is computed at runtime");
        return;
    }
    let program = normalize_program(first);
    ctx.acc.program(&program);
    dispatch(&program, args, ctx);
}

/// Skip leading `NAME=value` assignments (POSIX) and `$name = ...` (PowerShell).
fn skip_assignments(words: &[String], dialect: Dialect) -> &[String] {
    match dialect {
        Dialect::PowerShell => {
            if words.first().is_some_and(|word| word.starts_with('$')) {
                if words.get(1).is_some_and(|word| word == "=") {
                    return &words[2..];
                }
                if words.first().is_some_and(|word| word.ends_with('=')) {
                    return &words[1..];
                }
            }
            words
        }
        Dialect::Posix => {
            let skip = words.iter().take_while(|word| is_assignment(word)).count();
            &words[skip..]
        }
        Dialect::Cmd => words,
    }
}

pub(super) fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && !name.starts_with(|ch: char| ch.is_ascii_digit())
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

/// Basename, lowercased, without Windows executable extensions.
pub(crate) fn normalize_program(word: &str) -> String {
    let base = word.rsplit(['/', '\\']).next().unwrap_or(word);
    let lower = base.to_ascii_lowercase();
    for extension in [".exe", ".com", ".bat", ".cmd"] {
        if let Some(stem) = lower.strip_suffix(extension) {
            if !stem.is_empty() {
                return stem.to_owned();
            }
        }
    }
    lower
}

/// Case-insensitive membership of any argument in `names`.
pub(crate) fn has_flag(args: &[String], names: &[&str]) -> bool {
    args.iter()
        .any(|arg| names.iter().any(|name| arg.eq_ignore_ascii_case(name)))
}

/// Letters of POSIX short-option groups (`-rf` → `r`, `f`).
pub(crate) fn short_flags(args: &[String]) -> impl Iterator<Item = char> + '_ {
    args.iter()
        .filter(|arg| arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 1)
        .flat_map(|arg| arg.chars().skip(1))
}

/// Arguments that are not options (`-x`, `--x`, `/x` on Windows-style tools).
pub(crate) fn operands(args: &[String]) -> impl Iterator<Item = &String> {
    args.iter().filter(|arg| !arg.starts_with('-'))
}

/// Drop leading options; `with_value` options also consume the following word.
pub(super) fn skip_options<'a>(args: &'a [String], with_value: &[&str]) -> &'a [String] {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if !arg.starts_with('-') || arg == "-" {
            break;
        }
        index += 1;
        if arg == "--" {
            break;
        }
        if with_value.iter().any(|name| arg == name) {
            index += 1;
        }
    }
    args.get(index..).unwrap_or_default()
}

fn dispatch(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    if files::classify(program, args, ctx)
        || git::classify(program, args, ctx)
        || packages::classify(program, args, ctx)
        || net::classify(program, args, ctx)
        || interpreters::wrappers(program, args, ctx)
        || interpreters::interpreters(program, args, ctx)
        || system::classify(program, args, ctx)
    {
        return;
    }
    system::fallback(program, args, ctx);
}
