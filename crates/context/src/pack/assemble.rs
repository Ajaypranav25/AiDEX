//! The pack assembly pipeline: rules → explicit files → instructions → summaries →
//! memory → retrieved chunks, each under the strict total budget.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
};

use aidex_core::{EntityId, WorkspacePath};
use aidex_workspace::{Access, ConfinedRoot, WorkspaceError};

use super::{
    ContextItem, ContextPack, ContextSource, DropReason, DroppedItem, MAX_DROPPED_LISTED,
    MAX_EXPLICIT_PATHS, MAX_PACK_BUDGET, MAX_SUMMARIES, MAX_SUMMARY_BYTES, MAX_TASK_BYTES,
    PackRequest, Trust, render,
};
use crate::{
    ContextError, exclude,
    index::{
        ChunkRow, Index, IndexState, LineRange, chunk_at_line, chunk_by_ordinal, file_chunks,
        file_mtime_ns, lexical_chunks, query_terms, symbol_rows,
    },
    memory::{MemoryEntry, MemoryScope, MemorySource, MemoryStore},
    rules::ProjectRules,
    tokens::TokenEstimator,
};

/// Share of the budget instruction files may use.
const INSTRUCTIONS_SHARE: f64 = 0.30;
/// Share of the budget summaries may use.
const SUMMARY_SHARE: f64 = 0.25;
/// Share of the budget memories may use.
const MEMORY_SHARE: f64 = 0.15;
/// Lexical candidates fetched from the index.
const LEXICAL_CANDIDATES: usize = 64;
/// Ranked chunks considered for inclusion (before neighbour expansion).
const MAX_SEEDS: usize = 32;
/// Identifier-like task words looked up as symbols.
const MAX_SYMBOL_TERMS: usize = 16;
/// Definitions used per identifier.
const MAX_SYMBOL_MATCHES: usize = 8;
/// Score added to a chunk that contains a definition named in the task.
const SYMBOL_BOOST: f64 = 1.5;
/// Score of a neighbour relative to the chunk it extends.
const NEIGHBOUR_FACTOR: f64 = 0.5;

/// Function words left out of retrieval terms.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "then", "else", "of", "in", "on", "at", "to",
    "for", "from", "by", "with", "without", "into", "is", "are", "was", "were", "be", "been", "it",
    "its", "this", "that", "these", "those", "as", "so", "do", "does", "did", "not", "no", "can",
    "could", "should", "would", "will", "may", "might", "must", "we", "you", "i", "me", "my",
    "our", "your", "they", "them", "their", "what", "which", "who", "when", "where", "why", "how",
    "all", "any", "some", "there", "here", "about", "please", "make", "fix",
];

/// What a pack is assembled from. `index`, `rules`, and `memory` are optional so
/// callers can pack without them; `root` is used to check that indexed files are
/// still current before their text is packed (without a root, files are assumed
/// current).
pub struct PackSources<'a> {
    pub index: Option<&'a Index>,
    pub root: Option<&'a ConfinedRoot>,
    pub rules: Option<&'a ProjectRules>,
    pub memory: Option<&'a dyn MemoryStore>,
    pub estimator: &'a dyn TokenEstimator,
}

/// Assemble a context pack. The sum of item tokens never exceeds
/// `request.budget_tokens`.
pub fn assemble(
    sources: &PackSources<'_>,
    request: &PackRequest,
) -> Result<ContextPack, ContextError> {
    validate(request)?;
    let id = EntityId::generate("ctx").map_err(|error| ContextError::Index(error.to_string()))?;
    // The last 12 ULID characters are random (60 bits from the OS RNG).
    let ulid = id.as_str();
    let nonce = ulid[ulid.len().saturating_sub(12)..].to_owned();
    let mut packer = Packer {
        nonce,
        budget: request.budget_tokens,
        used: 0,
        estimator: sources.estimator,
        items: Vec::new(),
        dropped: Vec::new(),
        dropped_unlisted: 0,
    };
    let terms: Vec<String> = query_terms(&request.task_text)
        .into_iter()
        .filter(|term| !STOPWORDS.contains(&term.as_str()))
        .collect();
    let mut checker = Freshness::new(sources.root);

    let rules = sources.rules.filter(|_| request.include.rules);
    if let Some(summary) = rules.and_then(ProjectRules::summary_text) {
        let mut unlimited = usize::MAX;
        if let Err(tokens) = packer.try_add(
            ContextSource::Rule,
            Trust::Project,
            1.0,
            &summary,
            None,
            &mut unlimited,
        ) {
            packer.drop_item(ContextSource::Rule, DropReason::Budget, Some(tokens));
        }
    }

    let mut explicit_files = HashSet::new();
    for path in &request.include.explicit_paths {
        explicit_files.insert(path.as_str().to_owned());
        add_explicit(&mut packer, sources.index, &mut checker, path)?;
    }

    if let Some(rules) = rules {
        let mut cap = share(request.budget_tokens, INSTRUCTIONS_SHARE);
        for file in rules.instructions() {
            let source = ContextSource::ProjectInstructions {
                path: file.path.clone(),
            };
            let note = file
                .truncated
                .then(|| format!("file truncated: {} bytes total", file.bytes));
            add_lines(
                &mut packer,
                source,
                Trust::Project,
                &file.text,
                note,
                &mut cap,
            );
        }
    }

    let mut cap = share(request.budget_tokens, SUMMARY_SHARE);
    for (index, summary) in request.summaries.iter().enumerate() {
        let text = exclude::redact(summary);
        let source = ContextSource::Summary { index };
        if let Err(tokens) =
            packer.try_add(source.clone(), Trust::Untrusted, 1.0, &text, None, &mut cap)
        {
            packer.drop_item(source, DropReason::Budget, Some(tokens));
        }
    }

    if request.include.memory
        && let Some(store) = sources.memory
    {
        add_memories(&mut packer, store, request, &terms)?;
    }

    let index_state = match sources.index {
        Some(index) => index.summary()?.state,
        None => IndexState::Empty,
    };
    if request.include.search_hits
        && let Some(index) = sources.index
    {
        add_retrieved(
            &mut packer,
            index,
            &mut checker,
            &request.task_text,
            &terms,
            &explicit_files,
        )?;
    }

    let total_tokens = packer.items.iter().map(|item| item.tokens).sum();
    debug_assert!(total_tokens <= packer.budget && total_tokens == packer.used);
    Ok(ContextPack {
        id,
        nonce: packer.nonce,
        workspace_id: request.workspace_id.clone(),
        budget_tokens: request.budget_tokens,
        total_tokens,
        items: packer.items,
        dropped: packer.dropped,
        dropped_unlisted: packer.dropped_unlisted,
        terms,
        index_state,
    })
}

fn validate(request: &PackRequest) -> Result<(), ContextError> {
    let invalid = |message: String| Err(ContextError::InvalidRequest(message));
    if request.budget_tokens == 0 || request.budget_tokens > MAX_PACK_BUDGET {
        return invalid(format!(
            "budget_tokens must be between 1 and {MAX_PACK_BUDGET}"
        ));
    }
    if request.task_text.len() > MAX_TASK_BYTES {
        return invalid(format!(
            "the task text is longer than {MAX_TASK_BYTES} bytes"
        ));
    }
    if request.include.explicit_paths.len() > MAX_EXPLICIT_PATHS {
        return invalid(format!(
            "at most {MAX_EXPLICIT_PATHS} explicit paths are allowed"
        ));
    }
    if request.summaries.len() > MAX_SUMMARIES
        || request
            .summaries
            .iter()
            .any(|summary| summary.len() > MAX_SUMMARY_BYTES)
    {
        return invalid(format!(
            "at most {MAX_SUMMARIES} summaries of up to {MAX_SUMMARY_BYTES} bytes are allowed"
        ));
    }
    Ok(())
}

fn share(budget: usize, fraction: f64) -> usize {
    ((budget as f64) * fraction).floor() as usize
}

struct Packer<'a> {
    nonce: String,
    budget: usize,
    /// Tokens of every accepted item, including retrieved files not yet pushed.
    used: usize,
    estimator: &'a dyn TokenEstimator,
    items: Vec<ContextItem>,
    dropped: Vec<DroppedItem>,
    dropped_unlisted: usize,
}

impl Packer<'_> {
    fn remaining(&self) -> usize {
        self.budget.saturating_sub(self.used)
    }

    fn render(
        &self,
        source: &ContextSource,
        trust: Trust,
        content: &str,
        note: Option<&str>,
    ) -> (String, usize) {
        let text = render::wrap(&self.nonce, source, trust, content, note);
        let tokens = self.estimator.estimate(&text);
        (text, tokens)
    }

    /// Add the item if it fits both the budget and `cap`; otherwise return its cost.
    fn try_add(
        &mut self,
        source: ContextSource,
        trust: Trust,
        score: f64,
        content: &str,
        note: Option<&str>,
        cap: &mut usize,
    ) -> Result<(), usize> {
        let (text, tokens) = self.render(&source, trust, content, note);
        if tokens > self.remaining() || tokens > *cap {
            return Err(tokens);
        }
        *cap -= tokens;
        self.push(source, trust, score, text, tokens);
        Ok(())
    }

    fn push(
        &mut self,
        source: ContextSource,
        trust: Trust,
        score: f64,
        text: String,
        tokens: usize,
    ) {
        self.used += tokens;
        self.items.push(ContextItem {
            source,
            text,
            tokens,
            score,
            trust,
        });
    }

    fn drop_item(&mut self, source: ContextSource, reason: DropReason, tokens: Option<usize>) {
        if self.dropped.len() < MAX_DROPPED_LISTED {
            self.dropped.push(DroppedItem {
                source,
                reason,
                tokens,
            });
        } else {
            self.dropped_unlisted += 1;
        }
    }
}

/// The largest `count` in `1..=max` whose rendering costs at most `limit` tokens
/// (binary search; costs grow with `count`).
fn fit_prefix<T>(
    max: usize,
    limit: usize,
    mut attempt: impl FnMut(usize) -> (T, usize),
) -> Option<(usize, T, usize)> {
    let (mut low, mut high) = (0usize, max);
    let mut best = None;
    while low < high {
        let middle = (low + high).div_ceil(2);
        let (value, tokens) = attempt(middle);
        if tokens <= limit {
            best = Some((middle, value, tokens));
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    best
}

/// Add `content` whole if it fits, else its longest prefix of lines that fits
/// (noting how many lines were left out), else drop it.
fn add_lines(
    packer: &mut Packer<'_>,
    source: ContextSource,
    trust: Trust,
    content: &str,
    note: Option<String>,
    cap: &mut usize,
) {
    let limit = packer.remaining().min(*cap);
    let (text, tokens) = packer.render(&source, trust, content, note.as_deref());
    if tokens <= limit {
        *cap -= tokens;
        packer.push(source, trust, 1.0, text, tokens);
        return;
    }
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let fitted = fit_prefix(lines.len().saturating_sub(1), limit, |count| {
        let prefix: String = lines[..count].concat();
        let omitted = format!(
            "{} of {} lines omitted to fit the context budget",
            lines.len() - count,
            lines.len()
        );
        let note = match &note {
            Some(note) => format!("{note}; {omitted}"),
            None => omitted,
        };
        packer.render(&source, trust, &prefix, Some(&note))
    });
    match fitted {
        Some((_, text, fitted_tokens)) => {
            *cap -= fitted_tokens;
            packer.push(source, trust, 1.0, text, fitted_tokens);
        }
        None => packer.drop_item(source, DropReason::Budget, Some(tokens)),
    }
}

/// Whether an indexed file is still what the index recorded.
struct Freshness<'a> {
    root: Option<&'a ConfinedRoot>,
    cache: HashMap<String, Option<DropReason>>,
}

impl<'a> Freshness<'a> {
    fn new(root: Option<&'a ConfinedRoot>) -> Self {
        Self {
            root,
            cache: HashMap::new(),
        }
    }

    /// `None` when the file may be packed, else why not.
    fn check(&mut self, path: &str, size: u64, mtime_ns: i64) -> Option<DropReason> {
        if let Some(cached) = self.cache.get(path) {
            return *cached;
        }
        let verdict = self.compute(path, size, mtime_ns);
        self.cache.insert(path.to_owned(), verdict);
        verdict
    }

    fn compute(&self, path: &str, size: u64, mtime_ns: i64) -> Option<DropReason> {
        if exclude::is_secret_path(path) {
            return Some(DropReason::SecretFile);
        }
        let root = self.root?;
        let Ok(relative) = WorkspacePath::new(path) else {
            return Some(DropReason::Unreadable);
        };
        let absolute = match root.resolve(&relative, Access::Read) {
            Ok(absolute) => absolute,
            Err(WorkspaceError::PathNotFound(_)) => return Some(DropReason::Stale),
            Err(_) => return Some(DropReason::Unreadable),
        };
        let Ok(metadata) = fs::symlink_metadata(&absolute) else {
            return Some(DropReason::Stale);
        };
        if metadata.file_type().is_symlink() {
            return Some(DropReason::Unreadable);
        }
        if !metadata.is_file() || metadata.len() != size || file_mtime_ns(&metadata) != mtime_ns {
            return Some(DropReason::Stale);
        }
        None
    }
}

fn file_source(path: &str, lines: LineRange) -> Option<ContextSource> {
    Some(ContextSource::File {
        path: WorkspacePath::new(path).ok()?,
        lines,
    })
}

fn chunk_text(chunks: &[&ChunkRow]) -> String {
    let joined: String = chunks.iter().map(|chunk| chunk.text.as_str()).collect();
    exclude::redact(&joined).into_owned()
}

fn add_explicit(
    packer: &mut Packer<'_>,
    index: Option<&Index>,
    checker: &mut Freshness<'_>,
    path: &WorkspacePath,
) -> Result<(), ContextError> {
    let unindexed = ContextSource::File {
        path: path.clone(),
        lines: LineRange::new(1, 1),
    };
    if exclude::is_secret_path(path.as_str()) {
        packer.drop_item(unindexed, DropReason::SecretFile, None);
        return Ok(());
    }
    let Some(index) = index else {
        packer.drop_item(unindexed, DropReason::NotIndexed, None);
        return Ok(());
    };
    let chunks = index.with_reader(|connection| file_chunks(connection, path.as_str()))?;
    let (Some(first), Some(last)) = (chunks.first(), chunks.last()) else {
        packer.drop_item(unindexed, DropReason::NotIndexed, None);
        return Ok(());
    };
    let last_line = last.end_line;
    let all = ContextSource::File {
        path: path.clone(),
        lines: LineRange::new(1, last_line),
    };
    if let Some(reason) = checker.check(path.as_str(), first.size, first.mtime_ns) {
        packer.drop_item(all, reason, None);
        return Ok(());
    }
    let refs: Vec<&ChunkRow> = chunks.iter().collect();
    let content = chunk_text(&refs);
    let mut unlimited = usize::MAX;
    let Err(tokens) = packer.try_add(
        all.clone(),
        Trust::Untrusted,
        1.0,
        &content,
        None,
        &mut unlimited,
    ) else {
        return Ok(());
    };
    // Otherwise the largest prefix of whole chunks that fits.
    let limit = packer.remaining();
    let fitted = fit_prefix(refs.len() - 1, limit, |count| {
        let end = refs[count - 1].end_line;
        let source = ContextSource::File {
            path: path.clone(),
            lines: LineRange::new(1, end),
        };
        let note = format!(
            "lines {}-{last_line} omitted to fit the context budget",
            end + 1
        );
        let (text, tokens) = packer.render(
            &source,
            Trust::Untrusted,
            &chunk_text(&refs[..count]),
            Some(&note),
        );
        ((source, text, end), tokens)
    });
    match fitted {
        Some((_, (source, text, end), fitted_tokens)) => {
            packer.push(source, Trust::Untrusted, 1.0, text, fitted_tokens);
            packer.drop_item(
                ContextSource::File {
                    path: path.clone(),
                    lines: LineRange::new(end + 1, last_line),
                },
                DropReason::Budget,
                None,
            );
        }
        None => packer.drop_item(all, DropReason::Budget, Some(tokens)),
    }
    Ok(())
}

fn add_memories(
    packer: &mut Packer<'_>,
    store: &dyn MemoryStore,
    request: &PackRequest,
    terms: &[String],
) -> Result<(), ContextError> {
    let mut scopes = Vec::new();
    if let Some(session_id) = &request.session_id {
        scopes.push(MemoryScope::Session {
            session_id: session_id.clone(),
        });
    }
    scopes.push(MemoryScope::Workspace {
        workspace_id: request.workspace_id.clone(),
    });
    scopes.push(MemoryScope::User);
    let mut candidates: Vec<(f64, MemoryScope, MemoryEntry)> = Vec::new();
    for scope in scopes {
        for entry in store.list(&scope)? {
            let words: HashSet<String> = query_terms(&entry.text)
                .into_iter()
                .chain(entry.tags.iter().cloned())
                .collect();
            let overlap = terms.iter().filter(|term| words.contains(*term)).count();
            let score = if terms.is_empty() {
                0.0
            } else {
                overlap as f64 / terms.len() as f64
            };
            candidates.push((score, scope.clone(), entry));
        }
    }
    candidates.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| b.2.updated_at.cmp(&a.2.updated_at))
    });
    let mut cap = share(request.budget_tokens, MEMORY_SHARE);
    for (score, scope, entry) in candidates {
        let source = ContextSource::Memory {
            scope: scope.kind(),
            id: entry.id.clone(),
        };
        // Files can be edited by hand; never pack secret-like text.
        if exclude::contains_secret_like(&entry.text) {
            packer.drop_item(source, DropReason::SecretLike, None);
            continue;
        }
        let trust = match entry.source {
            MemorySource::User => Trust::User,
            MemorySource::Agent { .. } => Trust::Untrusted,
        };
        let mut content = entry.text.clone();
        if !entry.tags.is_empty() {
            content.push_str("\n(tags: ");
            content.push_str(&entry.tags.join(", "));
            content.push(')');
        }
        if let Err(tokens) = packer.try_add(source.clone(), trust, score, &content, None, &mut cap)
        {
            packer.drop_item(source, DropReason::Budget, Some(tokens));
        }
    }
    Ok(())
}

/// Identifier-shaped words of the task (`parse_config`, `Index::open` and its
/// segments, `fooBar`), at most [`MAX_SYMBOL_TERMS`].
fn identifiers(task: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    task.split(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == ':'))
        .map(|word| word.trim_matches(':'))
        .flat_map(|word| {
            let segments: Vec<&str> = if word.contains("::") {
                word.split("::").collect()
            } else {
                Vec::new()
            };
            std::iter::once(word).chain(segments)
        })
        .filter(|word| word.chars().count() >= 3 && word.chars().any(char::is_alphabetic))
        .filter(|word| !STOPWORDS.contains(&word.to_lowercase().as_str()))
        .filter(|word| seen.insert((*word).to_owned()))
        .take(MAX_SYMBOL_TERMS)
        .map(str::to_owned)
        .collect()
}

struct Candidate {
    chunk: ChunkRow,
    score: f64,
}

/// Selected chunks of one file and their current rendering.
struct FileSelection {
    best: f64,
    chunks: BTreeMap<u32, ChunkRow>,
    rendered: Vec<(ContextSource, String, usize)>,
    tokens: usize,
}

/// Render a file's selected chunks as one item per contiguous run.
fn render_file(
    packer: &Packer<'_>,
    path: &str,
    chunks: &BTreeMap<u32, ChunkRow>,
) -> Vec<(ContextSource, String, usize)> {
    let mut runs: Vec<Vec<&ChunkRow>> = Vec::new();
    for chunk in chunks.values() {
        match runs.last_mut() {
            Some(run)
                if run
                    .last()
                    .is_some_and(|last| last.ordinal + 1 == chunk.ordinal) =>
            {
                run.push(chunk);
            }
            _ => runs.push(vec![chunk]),
        }
    }
    runs.into_iter()
        .filter_map(|run| {
            let first = run.first()?;
            let last = run.last()?;
            let source = file_source(path, LineRange::new(first.start_line, last.end_line))?;
            let (text, tokens) = packer.render(&source, Trust::Untrusted, &chunk_text(&run), None);
            Some((source, text, tokens))
        })
        .collect()
}

/// Add `chunk` to its file's selection if the re-rendered file still fits.
fn try_select(
    packer: &mut Packer<'_>,
    files: &mut BTreeMap<String, FileSelection>,
    chunk: ChunkRow,
    score: f64,
) -> bool {
    let path = chunk.path.clone();
    let (current_tokens, mut chunks) = match files.get(&path) {
        Some(selection) => (selection.tokens, selection.chunks.clone()),
        None => (0, BTreeMap::new()),
    };
    if chunks.contains_key(&chunk.ordinal) {
        return true;
    }
    chunks.insert(chunk.ordinal, chunk);
    let rendered = render_file(packer, &path, &chunks);
    let tokens: usize = rendered.iter().map(|(_, _, tokens)| tokens).sum();
    // `used` already includes this file's current rendering.
    let used = packer.used - current_tokens + tokens;
    if used > packer.budget {
        return false;
    }
    packer.used = used;
    let selection = files.entry(path).or_insert(FileSelection {
        best: score,
        chunks: BTreeMap::new(),
        rendered: Vec::new(),
        tokens: 0,
    });
    selection.best = selection.best.max(score);
    selection.chunks = chunks;
    selection.rendered = rendered;
    selection.tokens = tokens;
    true
}

fn add_retrieved(
    packer: &mut Packer<'_>,
    index: &Index,
    checker: &mut Freshness<'_>,
    task: &str,
    terms: &[String],
    explicit: &HashSet<String>,
) -> Result<(), ContextError> {
    let mut candidates: HashMap<i64, Candidate> = HashMap::new();
    index.with_reader(|connection| {
        let lexical = lexical_chunks(connection, terms, true, None, LEXICAL_CANDIDATES)?;
        let best = lexical
            .iter()
            .map(|(_, score)| *score)
            .fold(0.0f64, f64::max);
        for (chunk, score) in lexical {
            let normalized = if best > 0.0 { score / best } else { 0.0 };
            candidates.insert(
                chunk.id,
                Candidate {
                    chunk,
                    score: normalized,
                },
            );
        }
        for name in identifiers(task) {
            let rows = symbol_rows(connection, &name, None)?;
            for row in rows
                .into_iter()
                .filter(|row| row.score >= 3.0)
                .take(MAX_SYMBOL_MATCHES)
            {
                let Some(chunk) = chunk_at_line(connection, row.file_id, row.line)? else {
                    continue;
                };
                candidates
                    .entry(chunk.id)
                    .and_modify(|candidate| candidate.score += SYMBOL_BOOST)
                    .or_insert(Candidate {
                        chunk,
                        score: SYMBOL_BOOST,
                    });
            }
        }
        Ok(())
    })?;
    let mut ranked: Vec<Candidate> = candidates.into_values().collect();
    ranked.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.chunk.path.cmp(&b.chunk.path))
            .then_with(|| a.chunk.ordinal.cmp(&b.chunk.ordinal))
    });
    ranked.truncate(MAX_SEEDS);

    let mut files: BTreeMap<String, FileSelection> = BTreeMap::new();
    for candidate in ranked {
        let chunk = candidate.chunk;
        if explicit.contains(&chunk.path) {
            continue;
        }
        let lines = LineRange::new(chunk.start_line, chunk.end_line);
        if let Some(reason) = checker.check(&chunk.path, chunk.size, chunk.mtime_ns) {
            if let Some(source) = file_source(&chunk.path, lines) {
                packer.drop_item(source, reason, None);
            }
            continue;
        }
        let path = chunk.path.clone();
        let alone = BTreeMap::from([(chunk.ordinal, chunk.clone())]);
        if !try_select(packer, &mut files, chunk, candidate.score)
            && let Some(source) = file_source(&path, lines)
        {
            let cost = render_file(packer, &path, &alone)
                .iter()
                .map(|(_, _, tokens)| tokens)
                .sum();
            packer.drop_item(source, DropReason::Budget, Some(cost));
        }
    }

    // Neighbourhood expansion: the chunks before and after each selected chunk.
    let mut by_score: Vec<(f64, String)> = files
        .iter()
        .map(|(path, selection)| (selection.best, path.clone()))
        .collect();
    by_score.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (best, path) in &by_score {
        let Some(selection) = files.get(path) else {
            continue;
        };
        let seeds: Vec<(i64, u32)> = selection
            .chunks
            .values()
            .map(|chunk| (chunk.file_id, chunk.ordinal))
            .collect();
        for (file_id, ordinal) in seeds {
            for neighbour in [ordinal.checked_sub(1), ordinal.checked_add(1)]
                .into_iter()
                .flatten()
            {
                let chunk = index
                    .with_reader(|connection| chunk_by_ordinal(connection, file_id, neighbour))?;
                if let Some(chunk) = chunk {
                    try_select(packer, &mut files, chunk, best * NEIGHBOUR_FACTOR);
                }
            }
        }
    }

    for (_, path) in by_score {
        let Some(selection) = files.remove(&path) else {
            continue;
        };
        for (source, text, tokens) in selection.rendered {
            packer.items.push(ContextItem {
                source,
                text,
                tokens,
                score: selection.best,
                trust: Trust::Untrusted,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_keep_code_shaped_words() {
        assert_eq!(
            identifiers("Fix the bug in parse_config where Index::open fails for fooBar"),
            [
                "bug",
                "parse_config",
                "Index::open",
                "Index",
                "open",
                "fails",
                "fooBar"
            ]
        );
    }

    #[test]
    fn prefixes_fit_by_binary_search() {
        let fitted = fit_prefix(10, 25, |count| (count, count * 5));
        assert_eq!(
            fitted.map(|(count, _, tokens)| (count, tokens)),
            Some((5, 25))
        );
        assert!(fit_prefix(10, 3, |count| (count, count * 5)).is_none());
        assert!(fit_prefix(0, 100, |count| (count, count)).is_none());
        assert_eq!(share(10, 0.3), 3);
        assert_eq!(share(1, 0.15), 0);
    }
}
