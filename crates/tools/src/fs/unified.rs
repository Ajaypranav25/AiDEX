//! Strict application of a single-file unified diff.
//!
//! Hunks must apply in order with exact context (compared without line
//! terminators). A hunk that does not match at its stated line is searched for
//! after the previous hunk, nearest first; no fuzz is applied. Inserted lines take
//! the file's line ending (CRLF if its first line uses CRLF).

/// One parsed hunk: old lines (context + removed) and new lines (context + added).
#[derive(Debug, Default)]
struct Hunk {
    old_start: usize,
    old: Vec<String>,
    new: Vec<String>,
    /// The new side's last line has no trailing newline.
    new_no_newline: bool,
}

fn parse_range(text: &str) -> Option<(usize, usize)> {
    let (start, count) = match text.split_once(',') {
        Some((start, count)) => (start, count),
        None => (text, "1"),
    };
    Some((start.parse().ok()?, count.parse().ok()?))
}

fn parse_header(line: &str) -> Option<(usize, usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(' ')?;
    let rest = rest.strip_prefix('+')?;
    let (new, _) = rest.split_once(" @@")?;
    let (old_start, old_count) = parse_range(old)?;
    let (_, new_count) = parse_range(new)?;
    Some((old_start, old_count, new_count))
}

fn parse(diff: &str) -> Result<Vec<Hunk>, (usize, String)> {
    let mut hunks = Vec::new();
    let mut lines = diff.lines().peekable();
    while let Some(line) = lines.next() {
        if !line.starts_with("@@") {
            // `diff`, `index`, `---`, `+++` and other headers are informational.
            continue;
        }
        let index = hunks.len();
        let (old_start, old_count, new_count) =
            parse_header(line).ok_or((index, format!("malformed hunk header `{line}`")))?;
        let mut hunk = Hunk {
            old_start,
            ..Hunk::default()
        };
        let (mut old_seen, mut new_seen) = (0, 0);
        let mut last_was_new = false;
        while old_seen < old_count || new_seen < new_count {
            let Some(body) = lines.next() else {
                return Err((index, "the hunk ends before its stated line counts".into()));
            };
            let (marker, text) = body.split_at(body.len().min(1));
            match marker {
                " " | "" => {
                    hunk.old.push(text.to_owned());
                    hunk.new.push(text.to_owned());
                    old_seen += 1;
                    new_seen += 1;
                    last_was_new = true;
                }
                "-" => {
                    hunk.old.push(text.to_owned());
                    old_seen += 1;
                    last_was_new = false;
                }
                "+" => {
                    hunk.new.push(text.to_owned());
                    new_seen += 1;
                    last_was_new = true;
                }
                "\\" => {}
                _ => return Err((index, format!("unexpected hunk line `{body}`"))),
            }
            if old_seen > old_count || new_seen > new_count {
                return Err((
                    index,
                    "the hunk has more lines than its header states".into(),
                ));
            }
        }
        if lines.peek().is_some_and(|next| next.starts_with('\\')) {
            lines.next();
            if last_was_new {
                hunk.new_no_newline = true;
            }
        }
        hunks.push(hunk);
    }
    if hunks.is_empty() {
        return Err((0, "the diff contains no hunks".into()));
    }
    Ok(hunks)
}

/// Split into (text, terminator) pairs.
fn split_lines(content: &str) -> Vec<(&str, &str)> {
    let mut lines = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        match rest.find('\n') {
            Some(end) => {
                let (line, tail) = rest.split_at(end + 1);
                let (text, ending) = match line.strip_suffix("\r\n") {
                    Some(text) => (text, "\r\n"),
                    None => (&line[..line.len() - 1], "\n"),
                };
                lines.push((text, ending));
                rest = tail;
            }
            None => {
                lines.push((rest, ""));
                rest = "";
            }
        }
    }
    lines
}

fn matches_at(lines: &[(&str, &str)], at: usize, old: &[String]) -> bool {
    at + old.len() <= lines.len()
        && old
            .iter()
            .zip(&lines[at..])
            .all(|(expected, (actual, _))| expected == actual)
}

/// Apply `diff` to `content`; errors carry the failing hunk index.
pub(super) fn apply(content: &str, diff: &str) -> Result<String, (usize, String)> {
    let hunks = parse(diff)?;
    let lines = split_lines(content);
    let ending = lines.first().map_or(
        "\n",
        |(_, ending)| if *ending == "\r\n" { "\r\n" } else { "\n" },
    );
    let mut output: Vec<(String, &str)> = Vec::with_capacity(lines.len());
    let mut cursor = 0usize;
    for (index, hunk) in hunks.iter().enumerate() {
        let expected = hunk.old_start.saturating_sub(1).max(cursor);
        let expected = if hunk.old.is_empty() && hunk.old_start > 0 {
            // Pure insertion after line `old_start`.
            hunk.old_start.max(cursor)
        } else {
            expected
        };
        let position = if matches_at(&lines, expected, &hunk.old) {
            Some(expected)
        } else {
            let candidates = cursor..=lines.len().saturating_sub(hunk.old.len());
            candidates
                .filter(|&at| matches_at(&lines, at, &hunk.old))
                .min_by_key(|&at| at.abs_diff(expected))
        };
        let Some(at) = position else {
            return Err((index, "its context does not match the file".into()));
        };
        output.extend(
            lines[cursor..at]
                .iter()
                .map(|(text, ending)| ((*text).to_owned(), *ending)),
        );
        let replaced = &lines[at..at + hunk.old.len()];
        for (offset, text) in hunk.new.iter().enumerate() {
            let line_ending = replaced
                .get(offset)
                .filter(|(old, _)| old == text)
                .map_or(ending, |(_, ending)| *ending);
            output.push((text.clone(), line_ending));
        }
        if hunk.new_no_newline {
            if let Some(last) = output.last_mut() {
                last.1 = "";
            }
        }
        cursor = at + hunk.old.len();
    }
    output.extend(
        lines[cursor..]
            .iter()
            .map(|(text, ending)| ((*text).to_owned(), *ending)),
    );
    let count = output.len();
    let mut result = String::with_capacity(content.len());
    for (index, (text, line_ending)) in output.into_iter().enumerate() {
        result.push_str(&text);
        // Only the final line may lack a terminator.
        if line_ending.is_empty() && index + 1 < count {
            result.push_str(ending);
        } else {
            result.push_str(line_ending);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::apply;

    #[test]
    fn applies_hunks_with_headers_and_offsets() {
        let content = "a\nb\nc\nd\ne\n";
        let diff = "--- a/f\n+++ b/f\n@@ -2,2 +2,2 @@\n b\n-c\n+C\n@@ -5,1 +5,2 @@\n e\n+f\n";
        assert_eq!(apply(content, diff).unwrap(), "a\nb\nC\nd\ne\nf\n");
        // Stated line numbers that are off still apply when context is unique.
        let shifted = "@@ -1,2 +1,2 @@\n c\n-d\n+D\n";
        assert_eq!(apply(content, shifted).unwrap(), "a\nb\nc\nD\ne\n");
    }

    #[test]
    fn preserves_crlf_and_missing_final_newline() {
        let content = "one\r\ntwo\r\nthree";
        let diff = "@@ -2,2 +2,3 @@\n two\n-three\n\\ No newline at end of file\n+THREE\n+four\n\\ No newline at end of file\n";
        assert_eq!(apply(content, diff).unwrap(), "one\r\ntwo\r\nTHREE\r\nfour");
    }

    #[test]
    fn mismatched_context_fails_with_the_hunk_index() {
        let content = "a\nb\n";
        let diff = "@@ -1,1 +1,1 @@\n-a\n+A\n@@ -2,1 +2,1 @@\n-x\n+y\n";
        assert_eq!(apply(content, diff).unwrap_err().0, 1);
        assert!(apply(content, "no hunks").is_err());
        assert!(apply(content, "@@ -1,3 +1,1 @@\n-a\n").is_err());
    }

    #[test]
    fn inserts_into_empty_files() {
        assert_eq!(apply("", "@@ -0,0 +1,2 @@\n+x\n+y\n").unwrap(), "x\ny\n");
    }
}
