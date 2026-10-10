//! Line-based chunking and language detection.

/// Lines after which a chunk ends at the next blank line.
const TARGET_LINES: usize = 40;
/// Lines after which a chunk always ends.
const MAX_LINES: usize = 60;
/// Bytes after which a chunk ends at the end of the current line.
const MAX_CHUNK_BYTES: usize = 3_000;
/// Longest single line kept in a chunk; longer lines (minified code) are cut.
pub(crate) const MAX_LINE_BYTES: usize = 2_000;
/// Marker appended to a cut line.
const LINE_CUT: &str = " [line truncated]\n";

/// One indexed span of a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Chunk {
    pub ordinal: u32,
    /// 1-based, inclusive.
    pub start_line: u32,
    /// 1-based, inclusive.
    pub end_line: u32,
    pub text: String,
}

/// Split `text` into chunks of about [`TARGET_LINES`] lines, preferring to end at a
/// blank line and never exceeding [`MAX_LINES`] lines or (except for one cut line)
/// [`MAX_CHUNK_BYTES`].
pub(crate) fn chunk_lines(text: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut start_line = 1u32;
    let mut lines_in_chunk = 0usize;
    let mut line_number = 0u32;
    for line in text.split_inclusive('\n') {
        line_number += 1;
        if lines_in_chunk == 0 {
            start_line = line_number;
        }
        push_line(&mut current, line);
        lines_in_chunk += 1;
        let blank = line.trim().is_empty();
        let full = lines_in_chunk >= MAX_LINES
            || current.len() >= MAX_CHUNK_BYTES
            || (lines_in_chunk >= TARGET_LINES && blank);
        if full {
            chunks.push(Chunk {
                ordinal: chunks.len() as u32,
                start_line,
                end_line: line_number,
                text: std::mem::take(&mut current),
            });
            lines_in_chunk = 0;
        }
    }
    if lines_in_chunk > 0 {
        chunks.push(Chunk {
            ordinal: chunks.len() as u32,
            start_line,
            end_line: line_number,
            text: current,
        });
    }
    chunks
}

fn push_line(out: &mut String, line: &str) {
    if line.len() <= MAX_LINE_BYTES {
        out.push_str(line);
        return;
    }
    let mut cut = MAX_LINE_BYTES;
    while !line.is_char_boundary(cut) {
        cut -= 1;
    }
    out.push_str(&line[..cut]);
    out.push_str(LINE_CUT);
}

/// The language of a file, from its name, for symbol extraction and display.
pub fn language_for(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    match name.as_str() {
        "dockerfile" => return Some("dockerfile"),
        "makefile" | "gnumakefile" => return Some("make"),
        "cmakelists.txt" => return Some("cmake"),
        _ => {}
    }
    let extension = name.rsplit_once('.')?.1;
    Some(match extension {
        "rs" => "rust",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" | "pyi" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "ipp" => "cpp",
        "java" => "java",
        "cs" => "csharp",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "rb" => "ruby",
        "php" => "php",
        "md" | "markdown" => "markdown",
        "toml" => "toml",
        "json" | "jsonc" => "json",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" | "zsh" => "shell",
        "ps1" | "psm1" => "powershell",
        "html" | "htm" => "html",
        "css" | "scss" | "less" => "css",
        "sql" => "sql",
        "txt" => "text",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_cover_every_line_in_order() {
        let text: String = (1..=130).map(|n| format!("line {n}\n")).collect();
        let chunks = chunk_lines(&text);
        assert_eq!(chunks.len(), 3);
        assert_eq!((chunks[0].start_line, chunks[0].end_line), (1, 60));
        assert_eq!((chunks[1].start_line, chunks[1].end_line), (61, 120));
        assert_eq!((chunks[2].start_line, chunks[2].end_line), (121, 130));
        let joined: String = chunks.iter().map(|chunk| chunk.text.as_str()).collect();
        assert_eq!(joined, text);
        assert!(
            chunks
                .iter()
                .enumerate()
                .all(|(i, c)| c.ordinal == i as u32)
        );
    }

    #[test]
    fn chunks_prefer_blank_lines_after_the_target() {
        let mut text = String::new();
        for n in 1..=45 {
            text.push_str(if n == 42 { "\n" } else { "code\n" });
        }
        let chunks = chunk_lines(&text);
        assert_eq!(chunks[0].end_line, 42);
        assert_eq!((chunks[1].start_line, chunks[1].end_line), (43, 45));
    }

    #[test]
    fn long_lines_are_cut_and_large_chunks_split() {
        let long = format!("{}\nshort\n", "é".repeat(5_000));
        let chunks = chunk_lines(&long);
        assert!(chunks[0].text.contains("[line truncated]"));
        assert!(chunks[0].text.len() < MAX_LINE_BYTES + 64);
        assert_eq!(chunks.last().unwrap().end_line, 2);
        assert!(chunk_lines("").is_empty());
        let no_newline = chunk_lines("a\nb");
        assert_eq!((no_newline[0].start_line, no_newline[0].end_line), (1, 2));
    }

    #[test]
    fn languages_come_from_names() {
        assert_eq!(language_for("src/lib.rs"), Some("rust"));
        assert_eq!(language_for("web/App.TSX"), Some("typescript"));
        assert_eq!(language_for("Dockerfile"), Some("dockerfile"));
        assert_eq!(language_for("a/b/c.hpp"), Some("cpp"));
        assert_eq!(language_for("LICENSE"), None);
    }
}
