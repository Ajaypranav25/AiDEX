//! Incremental, cancellable, batch-committed index builds.

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::Path,
    time::Instant,
};

use aidex_core::{Timestamp, WorkspacePath};
use aidex_workspace::Access;
use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use super::{
    BuildOutcome, IndexOptions, IndexPhase, IndexProgress, IndexReport, IndexTarget, chunk,
    meta_keys, repo, schema, symbols,
    walk::{self, Candidate, SkipCounts, WalkSettings},
};
use crate::{ContextError, exclude, fsio};

/// Bytes inspected for NUL when deciding whether a file is binary.
const BINARY_PROBE: usize = 8_000;

struct Existing {
    id: i64,
    size: u64,
    mtime_ns: i64,
    sha256: String,
}

enum FileOutcome {
    Unchanged,
    Touched,
    Indexed { added: bool, redacted: bool },
    /// Not indexable now; any stored row is removed.
    Skipped,
}

fn sql(context: &'static str) -> impl Fn(rusqlite::Error) -> ContextError {
    move |error| ContextError::sqlite(context, error)
}

pub(super) fn build(
    database: &Path,
    target: &IndexTarget,
    options: &IndexOptions,
    is_cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&IndexProgress),
) -> Result<IndexReport, ContextError> {
    if options.batch_size == 0 || options.max_files == 0 {
        return Err(ContextError::InvalidRequest(
            "batch_size and max_files must be at least 1".into(),
        ));
    }
    let started = Instant::now();
    let mut connection = schema::connect(database)?;
    let run = schema::meta_get(&connection, meta_keys::RUNS)?
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    schema::meta_set(&connection, meta_keys::RUNS, &run.to_string())?;
    schema::meta_set(&connection, meta_keys::STATE, "building")?;
    schema::meta_set(
        &connection,
        meta_keys::LAST_RUN,
        Timestamp::now().as_str(),
    )?;

    let mut report = IndexReport {
        outcome: BuildOutcome::Completed,
        run,
        files_seen: 0,
        added: 0,
        updated: 0,
        touched: 0,
        unchanged: 0,
        removed: 0,
        redacted: 0,
        skipped: SkipCounts::default(),
        truncated: false,
        elapsed_ms: 0,
    };
    let mut status = IndexProgress::default();
    progress(&status);

    let settings = WalkSettings {
        max_file_bytes: options.max_file_bytes,
        max_files: options.max_files,
        global_excludes: options.global_excludes,
    };
    let Some(walked) = walk::collect(&target.root, &settings, &mut report.skipped, is_cancelled)?
    else {
        return finish_cancelled(&connection, report, started);
    };
    report.files_seen = walked.files.len() as u64;
    report.truncated = walked.truncated;
    status.phase = IndexPhase::Indexing;
    status.total = report.files_seen;
    progress(&status);

    let existing = load_existing(&connection)?;
    let mut keep: HashSet<String> = HashSet::with_capacity(walked.files.len());
    let mut deleted: HashSet<i64> = HashSet::new();
    for batch in walked.files.chunks(options.batch_size) {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql("cannot start an index batch"))?;
        let mut cancelled = false;
        for candidate in batch {
            if is_cancelled() {
                cancelled = true;
                break;
            }
            let previous = existing.get(candidate.path.as_str());
            let outcome = index_file(&transaction, target, options, candidate, previous, &mut report.skipped)?;
            status.processed += 1;
            match outcome {
                FileOutcome::Unchanged => {
                    report.unchanged += 1;
                    keep.insert(candidate.path.as_str().to_owned());
                }
                FileOutcome::Touched => {
                    report.touched += 1;
                    keep.insert(candidate.path.as_str().to_owned());
                }
                FileOutcome::Indexed { added, redacted } => {
                    if added {
                        report.added += 1;
                    } else {
                        report.updated += 1;
                    }
                    report.redacted += u64::from(redacted);
                    status.changed += 1;
                    keep.insert(candidate.path.as_str().to_owned());
                }
                FileOutcome::Skipped => {
                    if let Some(previous) = previous {
                        delete_file(&transaction, previous.id)?;
                        deleted.insert(previous.id);
                        report.removed += 1;
                        status.removed += 1;
                    }
                }
            }
        }
        transaction
            .commit()
            .map_err(sql("cannot commit an index batch"))?;
        progress(&status);
        if cancelled {
            return finish_cancelled(&connection, report, started);
        }
    }

    status.phase = IndexPhase::Pruning;
    progress(&status);
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sql("cannot start pruning"))?;
    for (path, previous) in &existing {
        if !keep.contains(path) && !deleted.contains(&previous.id) {
            delete_file(&transaction, previous.id)?;
            report.removed += 1;
            status.removed += 1;
        }
    }
    let metadata = repo::read(target.repository.as_ref());
    let repository = serde_json::to_string(&metadata)
        .map_err(|error| ContextError::Index(format!("cannot encode repository metadata: {error}")))?;
    schema::meta_set(&transaction, meta_keys::REPOSITORY, &repository)?;
    schema::meta_set(
        &transaction,
        meta_keys::TRUNCATED,
        if report.truncated { "1" } else { "0" },
    )?;
    schema::meta_set(&transaction, meta_keys::STATE, "complete")?;
    schema::meta_set(
        &transaction,
        meta_keys::LAST_COMPLETE,
        Timestamp::now().as_str(),
    )?;
    transaction.commit().map_err(sql("cannot commit pruning"))?;
    status.phase = IndexPhase::Finished;
    progress(&status);
    report.elapsed_ms = elapsed_ms(started);
    Ok(report)
}

fn finish_cancelled(
    connection: &Connection,
    mut report: IndexReport,
    started: Instant,
) -> Result<IndexReport, ContextError> {
    schema::meta_set(connection, meta_keys::STATE, "partial")?;
    report.outcome = BuildOutcome::Cancelled;
    report.elapsed_ms = elapsed_ms(started);
    Ok(report)
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn load_existing(connection: &Connection) -> Result<HashMap<String, Existing>, ContextError> {
    let mut statement = connection
        .prepare("SELECT id, path, size, mtime_ns, sha256 FROM files")
        .map_err(sql("cannot read indexed files"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                Existing {
                    id: row.get(0)?,
                    size: row.get::<_, i64>(2)?.max(0) as u64,
                    mtime_ns: row.get(3)?,
                    sha256: row.get(4)?,
                },
            ))
        })
        .map_err(sql("cannot read indexed files"))?;
    rows.collect::<Result<HashMap<_, _>, _>>()
        .map_err(sql("cannot read indexed files"))
}

fn delete_contents(transaction: &Transaction<'_>, id: i64) -> Result<(), ContextError> {
    transaction
        .prepare_cached(
            "DELETE FROM vectors WHERE chunk_id IN (SELECT id FROM chunks WHERE file_id = ?1)",
        )
        .and_then(|mut statement| statement.execute([id]))
        .map_err(sql("cannot delete vectors"))?;
    transaction
        .prepare_cached("DELETE FROM symbols WHERE file_id = ?1")
        .and_then(|mut statement| statement.execute([id]))
        .map_err(sql("cannot delete symbols"))?;
    transaction
        .prepare_cached("DELETE FROM chunks WHERE file_id = ?1")
        .and_then(|mut statement| statement.execute([id]))
        .map_err(sql("cannot delete chunks"))?;
    Ok(())
}

fn delete_file(transaction: &Transaction<'_>, id: i64) -> Result<(), ContextError> {
    delete_contents(transaction, id)?;
    transaction
        .prepare_cached("DELETE FROM files WHERE id = ?1")
        .and_then(|mut statement| statement.execute([id]))
        .map_err(sql("cannot delete an indexed file"))?;
    Ok(())
}

/// Read a confined, regular, non-link file of at most `max` bytes.
fn read_file(
    target: &IndexTarget,
    path: &WorkspacePath,
    max: u64,
) -> Result<(Vec<u8>, fs::Metadata), ReadSkip> {
    let absolute = target
        .root
        .resolve(path, Access::Read)
        .map_err(|_| ReadSkip::Unreadable)?;
    let metadata = fs::symlink_metadata(&absolute).map_err(|_| ReadSkip::Unreadable)?;
    if walk::is_link(&metadata) {
        return Err(ReadSkip::Link);
    }
    if !metadata.is_file() {
        return Err(ReadSkip::Unreadable);
    }
    if metadata.len() > max {
        return Err(ReadSkip::TooLarge);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(&absolute)
        .and_then(|file| file.take(max + 1).read_to_end(&mut bytes))
        .map_err(|_| ReadSkip::Unreadable)?;
    if bytes.len() as u64 > max {
        return Err(ReadSkip::TooLarge);
    }
    Ok((bytes, metadata))
}

enum ReadSkip {
    Unreadable,
    Link,
    TooLarge,
}

fn index_file(
    transaction: &Transaction<'_>,
    target: &IndexTarget,
    options: &IndexOptions,
    candidate: &Candidate,
    previous: Option<&Existing>,
    skipped: &mut SkipCounts,
) -> Result<FileOutcome, ContextError> {
    if let Some(previous) = previous {
        if previous.size == candidate.size && previous.mtime_ns == candidate.mtime_ns {
            return Ok(FileOutcome::Unchanged);
        }
    }
    let (bytes, metadata) = match read_file(target, &candidate.path, options.max_file_bytes) {
        Ok(read) => read,
        Err(ReadSkip::Unreadable) => {
            skipped.unreadable += 1;
            return Ok(FileOutcome::Skipped);
        }
        Err(ReadSkip::Link) => {
            skipped.link += 1;
            return Ok(FileOutcome::Skipped);
        }
        Err(ReadSkip::TooLarge) => {
            skipped.too_large += 1;
            return Ok(FileOutcome::Skipped);
        }
    };
    let size = bytes.len() as u64;
    let mtime_ns = walk::mtime_ns(&metadata);
    let sha256 = fsio::sha256_hex(&bytes);
    if let Some(previous) = previous {
        if previous.sha256 == sha256 {
            transaction
                .prepare_cached("UPDATE files SET size = ?1, mtime_ns = ?2 WHERE id = ?3")
                .and_then(|mut statement| statement.execute(params![size as i64, mtime_ns, previous.id]))
                .map_err(sql("cannot update an indexed file"))?;
            return Ok(FileOutcome::Touched);
        }
    }
    if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        skipped.binary += 1;
        return Ok(FileOutcome::Skipped);
    }
    let Ok(text) = String::from_utf8(bytes) else {
        skipped.binary += 1;
        return Ok(FileOutcome::Skipped);
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if exclude::contains_private_key(text) {
        skipped.secret_content += 1;
        return Ok(FileOutcome::Skipped);
    }
    let language = chunk::language_for(candidate.path.as_str());
    let chunks = chunk::chunk_lines(text);
    let lines = chunks.last().map_or(0, |chunk| chunk.end_line);
    let mut redacted = false;
    let mut stored = Vec::with_capacity(chunks.len());
    let mut found = Vec::new();
    for chunk in chunks {
        let clean = match exclude::redact(&chunk.text) {
            std::borrow::Cow::Borrowed(_) => chunk.text,
            std::borrow::Cow::Owned(text) => {
                redacted = true;
                text
            }
        };
        symbols::extract(language, &clean, chunk.start_line, &mut found);
        stored.push((chunk.ordinal, chunk.start_line, chunk.end_line, clean));
    }
    let now = Timestamp::now().unix_millis();
    let (file_id, added) = match previous {
        Some(previous) => {
            delete_contents(transaction, previous.id)?;
            transaction
                .prepare_cached(
                    "UPDATE files SET size = ?1, mtime_ns = ?2, sha256 = ?3, language = ?4,
                     lines = ?5, redacted = ?6, indexed_at = ?7 WHERE id = ?8",
                )
                .and_then(|mut statement| {
                    statement.execute(params![
                        size as i64,
                        mtime_ns,
                        sha256,
                        language,
                        lines,
                        redacted,
                        now,
                        previous.id
                    ])
                })
                .map_err(sql("cannot update an indexed file"))?;
            (previous.id, false)
        }
        None => {
            transaction
                .prepare_cached(
                    "INSERT INTO files (path, size, mtime_ns, sha256, language, lines, redacted, indexed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .and_then(|mut statement| {
                    statement.execute(params![
                        candidate.path.as_str(),
                        size as i64,
                        mtime_ns,
                        sha256,
                        language,
                        lines,
                        redacted,
                        now
                    ])
                })
                .map_err(sql("cannot add an indexed file"))?;
            (transaction.last_insert_rowid(), true)
        }
    };
    {
        let mut insert = transaction
            .prepare_cached(
                "INSERT INTO chunks (file_id, ordinal, start_line, end_line, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(sql("cannot add chunks"))?;
        for (ordinal, start, end, text) in &stored {
            insert
                .execute(params![file_id, ordinal, start, end, text])
                .map_err(sql("cannot add chunks"))?;
        }
        let mut insert = transaction
            .prepare_cached("INSERT INTO symbols (file_id, name, kind, line) VALUES (?1, ?2, ?3, ?4)")
            .map_err(sql("cannot add symbols"))?;
        for symbol in &found {
            insert
                .execute(params![file_id, symbol.name, symbol.kind.as_str(), symbol.line])
                .map_err(sql("cannot add symbols"))?;
        }
    }
    Ok(FileOutcome::Indexed { added, redacted })
}
