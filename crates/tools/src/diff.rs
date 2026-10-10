//! Reviewable, bounded unified diffs for write, patch, and delete previews.

use std::time::Duration;

use aidex_core::WorkspacePath;
use serde::Serialize;
use similar::{ChangeTag, TextDiff};

/// Diffing stops refining after this long and reports a coarser (still correct) diff.
const DIFF_TIMEOUT: Duration = Duration::from_secs(2);
/// Bytes inspected for NUL when deciding whether content is binary.
pub(crate) const BINARY_SNIFF_BYTES: usize = 8 * 1024;

/// What a change does to one file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Created,
    Modified,
    Deleted,
}

impl DiffKind {
    pub fn file_change(self) -> aidex_events::FileChange {
        match self {
            Self::Created => aidex_events::FileChange::Created,
            Self::Modified => aidex_events::FileChange::Modified,
            Self::Deleted => aidex_events::FileChange::Deleted,
        }
    }
}

/// The reviewable change to one file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FileDiff {
    pub path: WorkspacePath,
    pub kind: DiffKind,
    /// Added lines (0 for binary content).
    pub additions: u64,
    /// Removed lines (0 for binary content).
    pub deletions: u64,
    /// Unified diff text (`a/` and `b/` prefixes); `None` for binary content or when
    /// the preview budget was spent on earlier files.
    pub unified: Option<String>,
    /// Either side contains NUL bytes or is not UTF-8.
    pub binary: bool,
    /// The unified text was cut at a line boundary to fit the preview budget.
    pub truncated: bool,
    pub before_sha256: Option<String>,
    pub after_sha256: Option<String>,
}

/// The diff a write/patch/delete plan will apply, shown before approval.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct DiffPreview {
    pub files: Vec<FileDiff>,
    /// More files change than are listed.
    pub files_omitted: u64,
}

impl DiffPreview {
    pub fn single(file: FileDiff) -> Self {
        Self {
            files: vec![file],
            files_omitted: 0,
        }
    }

    pub fn additions(&self) -> u64 {
        self.files.iter().map(|file| file.additions).sum()
    }

    pub fn deletions(&self) -> u64 {
        self.files.iter().map(|file| file.deletions).sum()
    }

    pub fn paths(&self) -> Vec<WorkspacePath> {
        self.files.iter().map(|file| file.path.clone()).collect()
    }
}

/// Whether `bytes` look binary: a NUL in the first [`BINARY_SNIFF_BYTES`].
pub(crate) fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0)
}

fn as_text(bytes: Option<&[u8]>) -> Option<&str> {
    match bytes {
        None => Some(""),
        Some(bytes) if looks_binary(bytes) => None,
        Some(bytes) => std::str::from_utf8(bytes).ok(),
    }
}

/// Diff `before` (None = absent) against `after` (None = absent), keeping at most
/// `budget` bytes of unified text (the budget is decremented).
pub(crate) fn file_diff(
    path: &WorkspacePath,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
    budget: &mut usize,
) -> FileDiff {
    let kind = match (before, after) {
        (None, _) => DiffKind::Created,
        (Some(_), None) => DiffKind::Deleted,
        (Some(_), Some(_)) => DiffKind::Modified,
    };
    let before_sha256 = before.map(crate::fs::sha256_hex);
    let after_sha256 = after.map(crate::fs::sha256_hex);
    let mut file = FileDiff {
        path: path.clone(),
        kind,
        additions: 0,
        deletions: 0,
        unified: None,
        binary: false,
        truncated: false,
        before_sha256,
        after_sha256,
    };
    let (Some(old), Some(new)) = (as_text(before), as_text(after)) else {
        file.binary = true;
        return file;
    };
    let diff = TextDiff::configure()
        .timeout(DIFF_TIMEOUT)
        .diff_lines(old, new);
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => file.additions += 1,
            ChangeTag::Delete => file.deletions += 1,
            ChangeTag::Equal => {}
        }
    }
    if file.additions == 0 && file.deletions == 0 {
        return file;
    }
    let old_header = match kind {
        DiffKind::Created => "/dev/null".to_owned(),
        _ => format!("a/{}", path.as_str()),
    };
    let new_header = match kind {
        DiffKind::Deleted => "/dev/null".to_owned(),
        _ => format!("b/{}", path.as_str()),
    };
    let text = diff
        .unified_diff()
        .context_radius(3)
        .header(&old_header, &new_header)
        .to_string();
    if *budget == 0 {
        file.truncated = true;
        return file;
    }
    let (kept, truncated) = cut_at_line(&text, *budget);
    *budget -= kept.len();
    file.truncated = truncated;
    file.unified = Some(kept.to_owned());
    file
}

/// The longest prefix of `text` that ends on a line boundary and fits `max` bytes.
pub(crate) fn cut_at_line(text: &str, max: usize) -> (&str, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut cut = max;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let prefix = &text[..cut];
    match prefix.rfind('\n') {
        Some(end) => (&text[..=end], true),
        None => (prefix, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wp(path: &str) -> WorkspacePath {
        WorkspacePath::new(path).unwrap()
    }

    #[test]
    fn modified_file_counts_lines_and_renders_headers() {
        let mut budget = 1024;
        let diff = file_diff(
            &wp("src/a.txt"),
            Some(b"one\ntwo\n"),
            Some(b"one\nTWO\nthree\n"),
            &mut budget,
        );
        assert_eq!(diff.kind, DiffKind::Modified);
        assert_eq!((diff.additions, diff.deletions), (2, 1));
        let text = diff.unified.unwrap();
        assert!(
            text.starts_with("--- a/src/a.txt\n+++ b/src/a.txt\n"),
            "{text}"
        );
        assert!(text.contains("-two\n+TWO\n+three\n"));
        assert!(!diff.truncated);
    }

    #[test]
    fn created_and_deleted_use_dev_null() {
        let mut budget = 1024;
        let created = file_diff(&wp("n.txt"), None, Some(b"x\n"), &mut budget);
        assert!(created.unified.unwrap().starts_with("--- /dev/null\n"));
        let deleted = file_diff(&wp("n.txt"), Some(b"x\n"), None, &mut budget);
        assert_eq!(deleted.kind, DiffKind::Deleted);
        assert!(deleted.unified.unwrap().contains("+++ /dev/null"));
        assert_eq!(deleted.deletions, 1);
    }

    #[test]
    fn binary_content_is_marked_without_text() {
        let mut budget = 1024;
        let diff = file_diff(&wp("b.bin"), Some(b"a\0b"), Some(b"c"), &mut budget);
        assert!(diff.binary);
        assert!(diff.unified.is_none());
        let invalid_utf8 = file_diff(&wp("b.bin"), None, Some(&[0xff, 0xfe]), &mut budget);
        assert!(invalid_utf8.binary);
    }

    #[test]
    fn unified_text_is_bounded_on_line_boundaries() {
        let new: String = (0..200).map(|line| format!("line {line}\n")).collect();
        let mut budget = 100;
        let diff = file_diff(&wp("big.txt"), None, Some(new.as_bytes()), &mut budget);
        let text = diff.unified.unwrap();
        assert!(diff.truncated);
        assert!(text.len() <= 100 && text.ends_with('\n'));
        assert_eq!(diff.additions, 200, "counts cover the whole change");
    }
}
