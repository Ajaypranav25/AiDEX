//! Human-readable rendering of workspace results (`--json` prints the DTOs).

use aidex_workspace::{
    ApplyReport, ChangeKind, Discovery, SkipReason, Vcs, Workspace, WorkspaceAvailability,
};
use aidexd::methods::workspace::{EphemeralReview, WorkspaceList};

fn status(workspace: &Workspace) -> String {
    match &workspace.availability {
        WorkspaceAvailability::Available => "available".into(),
        WorkspaceAvailability::Unavailable { reason } => format!("unavailable ({reason})"),
    }
}

fn kind(workspace: &Workspace) -> String {
    match &workspace.kind {
        aidex_workspace::WorkspaceKind::Ephemeral { source } => format!("ephemeral of {source}"),
        other => other.label().to_owned(),
    }
}

pub fn workspace_details(workspace: &Workspace, header: bool) {
    if header {
        println!("Workspace: {} ({})", workspace.name, workspace.id);
    }
    println!("Kind: {}", kind(workspace));
    println!("Path: {}", workspace.path);
    println!("Status: {}", status(workspace));
    if let Some(repository) = &workspace.repository {
        let relation = if repository.is_workspace_root {
            "root"
        } else {
            "inside"
        };
        println!("Repository: git ({relation}: {})", repository.root);
    }
    if header {
        println!("Created: {}", workspace.created_at.as_str());
        println!(
            "Last opened: {}",
            workspace
                .last_opened_at
                .as_ref()
                .map_or("never", |time| time.as_str())
        );
    }
}

pub fn workspace_table(list: &WorkspaceList) {
    if list.workspaces.is_empty() {
        println!("No workspaces registered. Add one with `aidex workspace add <PATH>`.");
        return;
    }
    for workspace in &list.workspaces {
        let marker = if list.selected.as_ref() == Some(&workspace.id) {
            "*"
        } else {
            " "
        };
        println!(
            "{marker} {}  {}  {}  {}  {}",
            workspace.id,
            workspace.name,
            kind(workspace),
            status(workspace),
            workspace.path
        );
    }
}

pub fn discovery(discovery: &Discovery) {
    println!("Selected root: {}", discovery.selected_root.display());
    match (&discovery.enclosing_repository, discovery.vcs) {
        (Some(repository), Vcs::Git) => println!("Repository: git at {}", repository.display()),
        _ => println!("Repository: none"),
    }
    if discovery.nested_repositories.is_empty() {
        println!("Nested repositories: none");
    } else {
        println!("Nested repositories:");
        for nested in &discovery.nested_repositories {
            println!("  {}", nested.as_str());
        }
    }
    if discovery.scan_truncated {
        println!("(scan stopped early; more nested repositories may exist)");
    }
}

pub fn skip_reason(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Link => "symbolic link or junction",
        SkipReason::UnsupportedName => "unsupported name",
        SkipReason::NotAFile => "not a regular file",
        SkipReason::Unreadable => "unreadable",
    }
}

fn letter(kind: ChangeKind) -> char {
    match kind {
        ChangeKind::Added => 'A',
        ChangeKind::Modified => 'M',
        ChangeKind::Deleted => 'D',
    }
}

/// Print the change list (and, with `diffs`, each unified diff).
pub fn review(review: &EphemeralReview, diffs: bool) {
    println!(
        "Ephemeral workspace {} (source {}): {} change(s), {} conflict(s)",
        review.workspace_id, review.source_id, review.total_changes, review.total_conflicts
    );
    for change in &review.changes {
        let mut notes = Vec::new();
        if change.binary {
            notes.push("binary".to_owned());
        }
        if let Some(conflict) = change.conflict {
            notes.push(format!("conflict: {}", conflict.label()));
        }
        let notes = if notes.is_empty() {
            String::new()
        } else {
            format!("  [{}]", notes.join(", "))
        };
        println!("{} {}{notes}", letter(change.kind), change.path.as_str());
    }
    for entry in &review.unsupported {
        println!(
            "! {}  [never applied: {}]",
            entry.path,
            skip_reason(entry.reason)
        );
    }
    if review.truncated {
        println!(
            "(output truncated; {} change(s) in total)",
            review.total_changes
        );
    }
    if diffs {
        for change in &review.changes {
            if let Some(diff) = &change.unified_diff {
                println!();
                print!("{diff}");
                if change.diff_truncated {
                    println!("... (diff truncated)");
                }
            }
        }
    }
    println!("Review digest: {}", review.digest);
}

pub fn apply_report(report: &ApplyReport) {
    for path in &report.applied {
        println!("applied  {}", path.as_str());
    }
    for path in &report.skipped_conflicts {
        println!("conflict {} (not applied)", path.as_str());
    }
    for failure in &report.failed {
        println!(
            "failed   {}: {} (not applied)",
            failure.path.as_str(),
            failure.message
        );
    }
    println!(
        "{} applied, {} skipped (conflict), {} failed",
        report.applied.len(),
        report.skipped_conflicts.len(),
        report.failed.len()
    );
}
