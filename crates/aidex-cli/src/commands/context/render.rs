//! Human-readable rendering of context results (`--json` prints the DTOs).

use aidex_context::{
    ContextPack, ContextSource, DroppedItem, IndexProgress, IndexStatus, MemoryEntry, MemorySource,
};
use aidexd::methods::{context::RulesResult, context::SearchResult, memory::MemoryList};

pub fn progress(progress: &IndexProgress) {
    println!(
        "  {:?}: {}/{} files compared, {} indexed, {} removed",
        progress.phase, progress.processed, progress.total, progress.changed, progress.removed
    );
}

pub fn status(status: &IndexStatus) {
    let index = &status.index;
    println!("Workspace: {}", status.workspace_id);
    println!("Index: {:?}", index.state);
    println!(
        "Files: {}  Chunks: {}  Symbols: {}  Size: {} bytes{}",
        index.files,
        index.chunks,
        index.symbols,
        index.bytes,
        if index.truncated {
            " (truncated at the file limit)"
        } else {
            ""
        }
    );
    if let Some(at) = &index.last_complete_at {
        println!("Last complete build: {}", at.as_str());
    }
    if let Some(repository) = &index.repository {
        let branch = repository
            .branch
            .as_deref()
            .unwrap_or(if repository.detached {
                "(detached)"
            } else {
                "(unknown)"
            });
        let head = repository
            .head
            .as_deref()
            .map_or("", |head| &head[..head.len().min(12)]);
        println!(
            "Repository: {branch} {head}  remotes: {}",
            repository.remotes.join(", ")
        );
    }
    if let Some(recovery) = &index.recovery {
        println!(
            "Recovered: {:?} ({}){}",
            recovery.kind,
            recovery.reason,
            recovery
                .moved_to
                .as_deref()
                .map(|moved| format!("; damaged copy kept as {moved}"))
                .unwrap_or_default()
        );
    }
    if let Some(job) = &status.job {
        println!("Job {}: {:?}", job.job_id, job.state);
        if let Some(report) = &job.report {
            println!(
                "  seen {}  added {}  updated {}  unchanged {}  touched {}  removed {}  redacted {}  ({} ms)",
                report.files_seen,
                report.added,
                report.updated,
                report.unchanged,
                report.touched,
                report.removed,
                report.redacted,
                report.elapsed_ms
            );
            let skipped = &report.skipped;
            if skipped.total() > 0 {
                println!(
                    "  skipped: {} secret names, {} secret contents, {} links, {} too large, {} binary, {} unsupported names, {} unreadable",
                    skipped.secret_name,
                    skipped.secret_content,
                    skipped.link,
                    skipped.too_large,
                    skipped.binary,
                    skipped.unsupported_name,
                    skipped.unreadable
                );
            }
        } else if job.error.is_none() {
            progress(&job.progress);
        }
        if let Some(error) = &job.error {
            println!("  error: {error}");
        }
    }
}

pub fn search(result: &SearchResult) {
    if result.hits.is_empty() {
        println!("No matches.");
    }
    for hit in &result.hits {
        let symbol = hit
            .symbol
            .as_ref()
            .map(|symbol| format!("  {} {}", symbol.kind, symbol.name))
            .unwrap_or_default();
        println!(
            "{}:{}-{}  (score {:.2}){symbol}",
            hit.path, hit.line_range.start, hit.line_range.end, hit.score
        );
        for line in hit.snippet.lines() {
            println!("    {line}");
        }
    }
    if !matches!(result.index_state, aidex_context::IndexState::Complete) {
        println!(
            "note: the index is {:?}; run `aidex index build` for complete results",
            result.index_state
        );
    }
}

fn source(source: &ContextSource) -> String {
    match source {
        ContextSource::File { path, lines } => {
            format!("file {}:{}-{}", path.as_str(), lines.start, lines.end)
        }
        ContextSource::Rule => "rules .aidex/rules.toml".to_owned(),
        ContextSource::Memory { scope, id } => format!("memory {scope} {id}"),
        ContextSource::Summary { index } => format!("summary #{index}"),
        ContextSource::ProjectInstructions { path } => format!("instructions {}", path.as_str()),
    }
}

fn dropped(item: &DroppedItem) -> String {
    let tokens = item
        .tokens
        .map(|tokens| format!(" ({tokens} tokens)"))
        .unwrap_or_default();
    format!("{} — {:?}{tokens}", source(&item.source), item.reason)
}

pub fn pack(pack: &ContextPack, text: bool) {
    println!(
        "Pack {}: {} of {} tokens in {} items (index {:?})",
        pack.id,
        pack.total_tokens,
        pack.budget_tokens,
        pack.items.len(),
        pack.index_state
    );
    println!("Terms: {}", pack.terms.join(" "));
    for item in &pack.items {
        println!(
            "  {:>6}  {:<9}  {:>5.2}  {}",
            item.tokens,
            item.trust.as_str(),
            item.score,
            source(&item.source)
        );
    }
    if !pack.dropped.is_empty() {
        println!("Dropped:");
        for item in &pack.dropped {
            println!("  {}", dropped(item));
        }
        if pack.dropped_unlisted > 0 {
            println!("  … and {} more", pack.dropped_unlisted);
        }
    }
    if text {
        println!();
        println!("{}", pack.preamble());
        println!();
        print!("{}", pack.render());
    }
}

pub fn memory(entry: &MemoryEntry) {
    let source = match &entry.source {
        MemorySource::User => "user".to_owned(),
        MemorySource::Agent { session_id } => format!("agent ({session_id})"),
    };
    let tags = if entry.tags.is_empty() {
        String::new()
    } else {
        format!("  [{}]", entry.tags.join(", "))
    };
    println!(
        "{}  {source}  {}{tags}",
        entry.id,
        entry.updated_at.as_str()
    );
    for line in entry.text.lines() {
        println!("    {line}");
    }
}

pub fn memories(list: &MemoryList) {
    if list.entries.is_empty() {
        println!("No memories.");
    }
    for entry in &list.entries {
        memory(entry);
    }
}

pub fn rules(result: &RulesResult) {
    let rules = &result.rules;
    println!("Workspace: {}", result.workspace_id);
    match &rules.source {
        Some(source) => println!("Rules: {source}"),
        None => println!("Rules: none (.aidex/rules.toml not found)"),
    }
    let project = &rules.project;
    for (label, value) in [
        ("Package manager", &project.package_manager),
        ("Tests", &project.tests),
        ("Lint", &project.lint),
        ("Build", &project.build),
    ] {
        if let Some(value) = value {
            println!("{label}: {value}");
        }
    }
    if !rules.never_modify.is_empty() {
        println!("Never modify: {}", rules.never_modify.join(", "));
    }
    if !rules.require_approval.is_empty() {
        println!("Require approval: {}", rules.require_approval.join(", "));
    }
    if rules.source.is_some() {
        println!("(Repository rules can only tighten policy.)");
    }
    for file in &rules.instructions {
        println!(
            "Instructions: {} ({} bytes{}{})",
            file.path.as_str(),
            file.bytes,
            if file.truncated { ", truncated" } else { "" },
            if file.redacted {
                ", credentials redacted"
            } else {
                ""
            }
        );
    }
}
