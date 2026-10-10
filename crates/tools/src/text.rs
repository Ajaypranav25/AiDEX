//! Bounded, redacted text for summaries, events, and process output.

use aidex_events::redact_text;

/// Redact credentials and cut to at most `max` bytes on a char boundary, appending
/// `…` when cut.
pub(crate) fn bounded(text: &str, max: usize) -> String {
    let redacted = redact_text(text);
    truncate(&redacted, max)
}

/// Cut to at most `max` bytes on a char boundary, appending `…` when cut.
pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut cut = max.saturating_sub('…'.len_utf8());
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &text[..cut])
}

/// Lossy UTF-8, redacted, for process output returned to callers.
pub(crate) fn process_text(bytes: &[u8]) -> String {
    let lossy = String::from_utf8_lossy(bytes);
    redact_text(&lossy).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_redacts_and_truncates() {
        let text = format!("push with ghp_{}", "a".repeat(36));
        let bounded = bounded(&text, 512);
        assert!(!bounded.contains("ghp_"), "{bounded}");
        let long = "é".repeat(100);
        let cut = truncate(&long, 11);
        assert!(cut.len() <= 11 && cut.ends_with('…'));
    }
}
