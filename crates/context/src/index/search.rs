//! Offline lexical (BM25), filename, and symbol search over an index.

use std::collections::HashSet;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::symbols::SymbolKind;
use crate::{ContextError, exclude};

/// Default number of hits.
pub const DEFAULT_SEARCH_LIMIT: usize = 20;
/// Most hits one search returns.
pub const MAX_SEARCH_LIMIT: usize = 200;
/// Longest accepted query text, in characters.
pub const MAX_QUERY_CHARS: usize = 512;
/// Most query terms used.
const MAX_TERMS: usize = 32;
/// Candidate rows ranked in memory for filename and symbol searches.
const MAX_CANDIDATES: i64 = 5_000;
/// Longest snippet line, in characters.
const SNIPPET_LINE_CHARS: usize = 240;

/// How a query is matched.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Full-text search over chunk text, ranked by BM25; every term must match
    /// (prefix matching for terms of three or more characters).
    #[default]
    Lexical,
    /// Case-insensitive substring match on workspace-relative paths.
    Filename,
    /// Case-insensitive match on definition names (exact > prefix > substring).
    Symbol,
}

/// A search request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub text: String,
    #[serde(default)]
    pub mode: SearchMode,
    /// Only paths equal to or beneath this workspace-relative directory or file.
    #[serde(default)]
    pub path_prefix: Option<String>,
    /// 1..=[`MAX_SEARCH_LIMIT`].
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    DEFAULT_SEARCH_LIMIT
}

impl Query {
    pub fn new(text: impl Into<String>, mode: SearchMode) -> Self {
        Self {
            text: text.into(),
            mode,
            path_prefix: None,
            limit: DEFAULT_SEARCH_LIMIT,
        }
    }

    pub fn lexical(text: impl Into<String>) -> Self {
        Self::new(text, SearchMode::Lexical)
    }

    pub fn filename(text: impl Into<String>) -> Self {
        Self::new(text, SearchMode::Filename)
    }

    pub fn symbol(text: impl Into<String>) -> Self {
        Self::new(text, SearchMode::Symbol)
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.path_prefix = Some(prefix.into());
        self
    }

    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// An inclusive, 1-based line range.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

impl LineRange {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// The definition a symbol hit matched.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolMatch {
    pub name: String,
    pub kind: SymbolKind,
}

/// One search result. Snippets come from the index (already redacted) and are
/// redacted again before they are returned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchHit {
    pub path: String,
    /// Lines the snippet shows.
    pub line_range: LineRange,
    pub snippet: String,
    /// Higher is better; comparable only within one result list.
    pub score: f64,
    pub language: Option<String>,
    pub symbol: Option<SymbolMatch>,
}

/// A stored chunk with its file's identity and freshness facts.
#[derive(Clone, Debug)]
pub(crate) struct ChunkRow {
    pub id: i64,
    pub file_id: i64,
    pub path: String,
    pub ordinal: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub text: String,
    pub size: u64,
    pub mtime_ns: i64,
}

const CHUNK_COLUMNS: &str = "c.id, c.file_id, f.path, c.ordinal, c.start_line, c.end_line, c.text, f.size, f.mtime_ns";

fn chunk_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChunkRow> {
    Ok(ChunkRow {
        id: row.get(0)?,
        file_id: row.get(1)?,
        path: row.get(2)?,
        ordinal: row.get(3)?,
        start_line: row.get(4)?,
        end_line: row.get(5)?,
        text: row.get(6)?,
        size: row.get::<_, i64>(7)?.max(0) as u64,
        mtime_ns: row.get(8)?,
    })
}

fn sql(context: &'static str) -> impl Fn(rusqlite::Error) -> ContextError {
    move |error| ContextError::sqlite(context, error)
}

/// Lowercased alphanumeric terms of `text`, deduplicated, in order.
pub(crate) fn query_terms(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .filter(|term| seen.insert(term.clone()))
        .take(MAX_TERMS)
        .collect()
}

/// An FTS5 query of quoted terms (no operator injection), joined by AND or OR.
pub(crate) fn fts_query(terms: &[String], any: bool) -> String {
    let parts: Vec<String> = terms
        .iter()
        .map(|term| {
            let quoted = format!("\"{}\"", term.replace('"', "\"\""));
            if term.chars().count() >= 3 {
                format!("{quoted}*")
            } else {
                quoted
            }
        })
        .collect();
    parts.join(if any { " OR " } else { " " })
}

/// Escape `%`, `_`, and `\` for a `LIKE … ESCAPE '\'` pattern.
fn like_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Normalized path prefix: `./src/` → `src`; `None` for an empty prefix.
pub(crate) fn normalize_prefix(prefix: Option<&str>) -> Result<Option<String>, ContextError> {
    let Some(prefix) = prefix else {
        return Ok(None);
    };
    let trimmed = prefix.trim().trim_start_matches("./").trim_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        return Ok(None);
    }
    aidex_core::WorkspacePath::new(trimmed)
        .map(|path| Some(path.as_str().to_owned()))
        .map_err(|error| ContextError::InvalidQuery(format!("path_prefix: {error}")))
}

/// SQL filter for `f.path` given parameters `?P` (exact) and `?L` (LIKE pattern).
fn prefix_params(prefix: Option<&str>) -> (Option<String>, Option<String>) {
    match prefix {
        Some(prefix) => (
            Some(prefix.to_owned()),
            Some(format!("{}/%", like_escape(prefix))),
        ),
        None => (None, None),
    }
}

fn validate(query: &Query) -> Result<(String, Option<String>), ContextError> {
    let text = query.text.trim();
    if text.is_empty() {
        return Err(ContextError::InvalidQuery("the query is empty".into()));
    }
    if text.chars().count() > MAX_QUERY_CHARS {
        return Err(ContextError::InvalidQuery(format!(
            "the query is longer than {MAX_QUERY_CHARS} characters"
        )));
    }
    if query.limit == 0 || query.limit > MAX_SEARCH_LIMIT {
        return Err(ContextError::InvalidQuery(format!(
            "limit must be between 1 and {MAX_SEARCH_LIMIT}"
        )));
    }
    Ok((text.to_owned(), normalize_prefix(query.path_prefix.as_deref())?))
}

pub(super) fn search(connection: &Connection, query: &Query) -> Result<Vec<SearchHit>, ContextError> {
    let (text, prefix) = validate(query)?;
    let mut hits = match query.mode {
        SearchMode::Lexical => lexical(connection, &text, prefix.as_deref(), query.limit)?,
        SearchMode::Filename => filename(connection, &text, prefix.as_deref(), query.limit)?,
        SearchMode::Symbol => symbol(connection, &text, prefix.as_deref(), query.limit)?,
    };
    hits.retain(|hit| !exclude::is_secret_path(&hit.path));
    for hit in &mut hits {
        if let std::borrow::Cow::Owned(clean) = exclude::redact(&hit.snippet) {
            hit.snippet = clean;
        }
    }
    Ok(hits)
}

/// Chunks matching `terms`, best first, with their BM25 score (higher is better).
pub(crate) fn lexical_chunks(
    connection: &Connection,
    terms: &[String],
    any: bool,
    prefix: Option<&str>,
    limit: usize,
) -> Result<Vec<(ChunkRow, f64)>, ContextError> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let (exact, pattern) = prefix_params(prefix);
    let mut statement = connection
        .prepare_cached(&format!(
            "SELECT {CHUNK_COLUMNS}, bm25(chunks_fts) AS rank
             FROM chunks_fts
             JOIN chunks c ON c.id = chunks_fts.rowid
             JOIN files f ON f.id = c.file_id
             WHERE chunks_fts MATCH ?1
               AND (?2 IS NULL OR f.path = ?2 OR f.path LIKE ?3 ESCAPE '\\')
             ORDER BY rank, c.id
             LIMIT ?4"
        ))
        .map_err(sql("cannot prepare a lexical search"))?;
    let rows = statement
        .query_map(
            params![fts_query(terms, any), exact, pattern, limit as i64],
            |row| Ok((chunk_row(row)?, -row.get::<_, f64>(9)?)),
        )
        .map_err(sql("lexical search failed"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql("lexical search failed"))
}

fn lexical(
    connection: &Connection,
    text: &str,
    prefix: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchHit>, ContextError> {
    let terms = query_terms(text);
    if terms.is_empty() {
        return Err(ContextError::InvalidQuery(
            "the query has no searchable words".into(),
        ));
    }
    let language = language_lookup(connection)?;
    Ok(lexical_chunks(connection, &terms, false, prefix, limit)?
        .into_iter()
        .map(|(chunk, score)| {
            let (line_range, snippet) = snippet(&chunk.text, chunk.start_line, &terms);
            SearchHit {
                language: language(&chunk.path),
                path: chunk.path,
                line_range,
                snippet,
                score,
                symbol: None,
            }
        })
        .collect())
}

/// A closure that looks a file's language up by path (best effort).
fn language_lookup(
    connection: &Connection,
) -> Result<impl Fn(&str) -> Option<String> + '_, ContextError> {
    Ok(move |path: &str| {
        connection
            .query_row(
                "SELECT language FROM files WHERE path = ?1",
                [path],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten()
    })
}

/// The first line containing a term, with one line of context on each side.
pub(crate) fn snippet(text: &str, start_line: u32, terms: &[String]) -> (LineRange, String) {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return (LineRange::new(start_line, start_line), String::new());
    }
    let found = lines
        .iter()
        .position(|line| {
            let lower = line.to_lowercase();
            terms.iter().any(|term| lower.contains(term.as_str()))
        })
        .unwrap_or(0);
    let first = found.saturating_sub(1);
    let last = (found + 1).min(lines.len() - 1);
    let snippet = lines[first..=last]
        .iter()
        .map(|line| cut(line))
        .collect::<Vec<_>>()
        .join("\n");
    (
        LineRange::new(start_line + first as u32, start_line + last as u32),
        snippet,
    )
}

fn cut(line: &str) -> String {
    if line.chars().count() <= SNIPPET_LINE_CHARS {
        line.to_owned()
    } else {
        let mut out: String = line.chars().take(SNIPPET_LINE_CHARS).collect();
        out.push('…');
        out
    }
}

fn filename(
    connection: &Connection,
    text: &str,
    prefix: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchHit>, ContextError> {
    let needle = text.to_lowercase();
    let (exact, pattern) = prefix_params(prefix);
    let mut statement = connection
        .prepare_cached(
            "SELECT f.id, f.path, f.language, f.lines FROM files f
             WHERE f.path LIKE ?1 ESCAPE '\\'
               AND (?2 IS NULL OR f.path = ?2 OR f.path LIKE ?3 ESCAPE '\\')
             ORDER BY f.path LIMIT ?4",
        )
        .map_err(sql("cannot prepare a filename search"))?;
    let rows = statement
        .query_map(
            params![
                format!("%{}%", like_escape(&needle)),
                exact,
                pattern,
                MAX_CANDIDATES
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, u32>(3)?,
                ))
            },
        )
        .map_err(sql("filename search failed"))?;
    let mut ranked = Vec::new();
    for row in rows {
        let (id, path, language, lines) = row.map_err(sql("filename search failed"))?;
        let lower = path.to_lowercase();
        let name = lower.rsplit('/').next().unwrap_or(&lower);
        let stem = name.split('.').next().unwrap_or(name);
        let score = if name == needle || stem == needle {
            4.0
        } else if name.starts_with(&needle) {
            3.0
        } else if name.contains(&needle) {
            2.0
        } else if lower.contains(&needle) {
            1.0
        } else {
            continue;
        };
        // Shorter paths first among equals.
        let score = score + 1.0 / (1.0 + path.len() as f64);
        ranked.push((score, id, path, language, lines));
    }
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
    ranked.truncate(limit);
    let mut hits = Vec::with_capacity(ranked.len());
    for (score, id, path, language, lines) in ranked {
        let head: Option<String> = connection
            .query_row(
                "SELECT text FROM chunks WHERE file_id = ?1 AND ordinal = 0",
                [id],
                |row| row.get(0),
            )
            .ok();
        let head = head.unwrap_or_default();
        let shown: Vec<String> = head.lines().take(3).map(cut).collect();
        let end = (shown.len() as u32).clamp(1, lines.max(1));
        hits.push(SearchHit {
            path,
            line_range: LineRange::new(1, end),
            snippet: shown.join("\n"),
            score,
            language,
            symbol: None,
        });
    }
    Ok(hits)
}

/// A definition matched by name.
#[derive(Clone, Debug)]
pub(crate) struct SymbolRow {
    pub name: String,
    pub kind: SymbolKind,
    pub line: u32,
    pub file_id: i64,
    pub path: String,
    pub language: Option<String>,
    /// 4 exact, 3 case-insensitive (or last `::` segment), 2 prefix, 1 substring.
    pub score: f64,
}

/// Definitions named like `name`, best first.
pub(crate) fn symbol_rows(
    connection: &Connection,
    name: &str,
    prefix: Option<&str>,
) -> Result<Vec<SymbolRow>, ContextError> {
    let (exact, pattern) = prefix_params(prefix);
    let mut statement = connection
        .prepare_cached(
            "SELECT s.name, s.kind, s.line, f.id, f.path, f.language
             FROM symbols s JOIN files f ON f.id = s.file_id
             WHERE s.name LIKE ?1 ESCAPE '\\'
               AND (?2 IS NULL OR f.path = ?2 OR f.path LIKE ?3 ESCAPE '\\')
             ORDER BY f.path, s.line LIMIT ?4",
        )
        .map_err(sql("cannot prepare a symbol search"))?;
    let rows = statement
        .query_map(
            params![
                format!("%{}%", like_escape(name)),
                exact,
                pattern,
                MAX_CANDIDATES
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .map_err(sql("symbol search failed"))?;
    let lower = name.to_lowercase();
    let mut ranked = Vec::new();
    for row in rows {
        let (found, kind, line, file_id, path, language) = row.map_err(sql("symbol search failed"))?;
        let Ok(kind) = kind.parse::<SymbolKind>() else {
            continue;
        };
        let found_lower = found.to_lowercase();
        let base = found_lower.rsplit("::").next().unwrap_or(&found_lower);
        let score = if found == name {
            4.0
        } else if found_lower == lower || base == lower {
            3.0
        } else if found_lower.starts_with(&lower) || base.starts_with(&lower) {
            2.0
        } else {
            1.0
        };
        ranked.push(SymbolRow {
            name: found,
            kind,
            line,
            file_id,
            path,
            language,
            score,
        });
    }
    ranked.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.cmp(&b.line))
    });
    Ok(ranked)
}

/// The chunk of `file_id` that contains `line`.
pub(crate) fn chunk_at_line(
    connection: &Connection,
    file_id: i64,
    line: u32,
) -> Result<Option<ChunkRow>, ContextError> {
    let mut statement = connection
        .prepare_cached(&format!(
            "SELECT {CHUNK_COLUMNS} FROM chunks c JOIN files f ON f.id = c.file_id
             WHERE c.file_id = ?1 AND c.start_line <= ?2 AND c.end_line >= ?2
             ORDER BY c.ordinal LIMIT 1"
        ))
        .map_err(sql("cannot prepare a chunk lookup"))?;
    let mut rows = statement
        .query_map(params![file_id, line], chunk_row)
        .map_err(sql("chunk lookup failed"))?;
    rows.next()
        .transpose()
        .map_err(sql("chunk lookup failed"))
}

/// The chunk of `file_id` with `ordinal`.
pub(crate) fn chunk_by_ordinal(
    connection: &Connection,
    file_id: i64,
    ordinal: u32,
) -> Result<Option<ChunkRow>, ContextError> {
    let mut statement = connection
        .prepare_cached(&format!(
            "SELECT {CHUNK_COLUMNS} FROM chunks c JOIN files f ON f.id = c.file_id
             WHERE c.file_id = ?1 AND c.ordinal = ?2"
        ))
        .map_err(sql("cannot prepare a chunk lookup"))?;
    let mut rows = statement
        .query_map(params![file_id, ordinal], chunk_row)
        .map_err(sql("chunk lookup failed"))?;
    rows.next()
        .transpose()
        .map_err(sql("chunk lookup failed"))
}

/// Every chunk of the file at `path`, in order (empty when not indexed).
pub(crate) fn file_chunks(connection: &Connection, path: &str) -> Result<Vec<ChunkRow>, ContextError> {
    let mut statement = connection
        .prepare_cached(&format!(
            "SELECT {CHUNK_COLUMNS} FROM chunks c JOIN files f ON f.id = c.file_id
             WHERE f.path = ?1 ORDER BY c.ordinal"
        ))
        .map_err(sql("cannot prepare a file lookup"))?;
    let rows = statement
        .query_map([path], chunk_row)
        .map_err(sql("file lookup failed"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql("file lookup failed"))
}

fn symbol(
    connection: &Connection,
    text: &str,
    prefix: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchHit>, ContextError> {
    let mut hits = Vec::new();
    for row in symbol_rows(connection, text, prefix)?.into_iter().take(limit) {
        let snippet = chunk_at_line(connection, row.file_id, row.line)?
            .and_then(|chunk| {
                let offset = row.line.checked_sub(chunk.start_line)? as usize;
                chunk.text.lines().nth(offset).map(cut)
            })
            .unwrap_or_default();
        hits.push(SearchHit {
            path: row.path,
            line_range: LineRange::new(row.line, row.line),
            snippet,
            score: row.score,
            language: row.language,
            symbol: Some(SymbolMatch {
                name: row.name,
                kind: row.kind,
            }),
        });
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_split_lowercased_and_deduplicated() {
        assert_eq!(
            query_terms("Parse parse_config, CONFIG! ok"),
            ["parse", "config", "ok"]
        );
        assert!(query_terms("  !!! ").is_empty());
    }

    #[test]
    fn fts_queries_quote_terms_and_add_prefixes() {
        let terms = vec!["ab".to_owned(), "config".to_owned()];
        assert_eq!(fts_query(&terms, false), "\"ab\" \"config\"*");
        assert_eq!(fts_query(&terms, true), "\"ab\" OR \"config\"*");
    }

    #[test]
    fn prefixes_are_normalized_and_validated() {
        assert_eq!(normalize_prefix(Some("./src/")).unwrap().as_deref(), Some("src"));
        assert_eq!(normalize_prefix(Some("")).unwrap(), None);
        assert!(normalize_prefix(Some("../etc")).is_err());
        assert_eq!(like_escape("a_b%c\\"), "a\\_b\\%c\\\\");
    }

    #[test]
    fn snippets_center_on_the_first_match() {
        let text = "one\ntwo\nthree needle\nfour\nfive\n";
        let (range, snippet) = snippet(text, 10, &["needle".to_owned()]);
        assert_eq!(range, LineRange::new(11, 13));
        assert_eq!(snippet, "two\nthree needle\nfour");
    }
}
