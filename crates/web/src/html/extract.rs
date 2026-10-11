//! HTML to readable text: headings as `#`, paragraphs, lists, fenced
//! `pre` blocks, inline code, tables as `a | b` rows, and links as
//! `[text](url)`; scripts, styles, and hidden or non-text elements are
//! dropped.

use url::Url;

use super::{
    entities,
    tokenizer::{Token, Tokenizer},
};

/// Elements whose content is dropped (their start and end tags are tracked).
const DROPPED: &[&str] = &[
    "noscript", "template", "svg", "math", "object", "canvas", "select", "datalist", "audio",
    "video", "map",
];
/// Inline elements that do not separate words in [`plain_text`].
const INLINE: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "cite", "code", "data", "dfn", "em", "i", "kbd", "mark",
    "q", "s", "samp", "small", "span", "strong", "sub", "sup", "time", "u", "var", "wbr",
];
/// Raw-text elements whose content is dropped (`title` is captured instead).
const RAW_DROPPED: &[&str] = &[
    "script", "style", "textarea", "xmp", "iframe", "noembed", "noframes",
];
/// Elements that never have content or an end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
    "source", "track", "wbr",
];
/// Elements that start and end a block (a blank line around them).
const BLOCKS: &[&str] = &[
    "p", "div", "section", "article", "main", "header", "footer", "nav", "aside",
    "blockquote", "figure", "figcaption", "table", "form", "fieldset", "address", "details",
    "summary", "dl", "body", "center", "caption", "legend",
];
/// Longest title kept, in characters.
const MAX_TITLE_CHARS: usize = 512;

/// Readable text extracted from an HTML document.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Readable {
    pub title: Option<String>,
    pub text: String,
    /// The text reached the size limit and was cut.
    pub truncated: bool,
}

struct ListFrame {
    ordered: bool,
    counter: u32,
}

struct LinkFrame {
    href: Option<String>,
    start: usize,
}

struct Extractor {
    out: String,
    max: usize,
    truncated: bool,
    pending_space: bool,
    base: Option<Url>,
    title: Option<String>,
    capture_title: bool,
    raw: Option<String>,
    skip: Vec<String>,
    lists: Vec<ListFrame>,
    links: Vec<LinkFrame>,
    pre: usize,
    code: usize,
    cell: usize,
}

/// Extract readable text from `html`, resolving relative links against
/// `base`, keeping at most `max_bytes` of text.
pub fn extract(html: &str, base: Option<&Url>, max_bytes: usize) -> Readable {
    let mut extractor = Extractor {
        out: String::new(),
        max: max_bytes,
        truncated: false,
        pending_space: false,
        base: base.cloned(),
        title: None,
        capture_title: false,
        raw: None,
        skip: Vec::new(),
        lists: Vec::new(),
        links: Vec::new(),
        pre: 0,
        code: 0,
        cell: 0,
    };
    for token in Tokenizer::new(html) {
        if extractor.truncated {
            break;
        }
        extractor.token(&token);
    }
    let text = tidy(&extractor.out);
    Readable {
        title: extractor.title,
        text,
        truncated: extractor.truncated,
    }
}

/// Collapse whitespace in `text` (entity-decoded, tags removed) and cut it to
/// `max_chars` characters with an ellipsis.
pub fn plain_text(fragment: &str, max_chars: usize) -> String {
    let mut words = String::new();
    for token in Tokenizer::new(fragment) {
        match token {
            Token::Text(text) => words.push_str(&entities::decode(text)),
            Token::Start { name, .. } | Token::End { name } => {
                if !INLINE.contains(&name.as_str()) {
                    words.push(' ');
                }
            }
        }
    }
    let collapsed = words.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&collapsed, max_chars)
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        None => text.to_owned(),
        Some((cut, _)) => {
            let mut short = text[..cut].trim_end().to_owned();
            short.push('\u{2026}');
            short
        }
    }
}

/// Trim trailing spaces on every line, keep at most one blank line in a row,
/// and trim the ends.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_owned()
}

impl Extractor {
    fn token(&mut self, token: &Token<'_>) {
        match token {
            Token::Start {
                name, self_closing, ..
            } => self.start(name, token, *self_closing),
            Token::End { name } => self.end(name),
            Token::Text(text) => self.text(text),
        }
    }

    fn start(&mut self, name: &str, token: &Token<'_>, self_closing: bool) {
        let void = VOID.contains(&name) || self_closing;
        if !self.skip.is_empty() {
            if !void {
                self.skip.push(name.to_owned());
            }
            return;
        }
        if name == "title" {
            self.capture_title = self.title.is_none() && !self_closing;
            self.raw = Some(name.to_owned());
            return;
        }
        if RAW_DROPPED.contains(&name) {
            if !self_closing {
                self.raw = Some(name.to_owned());
            }
            return;
        }
        let hidden = token.attribute("hidden").is_some()
            || token
                .attribute("aria-hidden")
                .is_some_and(|value| value.eq_ignore_ascii_case("true"));
        if (DROPPED.contains(&name) || hidden) && !void {
            self.skip.push(name.to_owned());
            return;
        }
        match name {
            "base" => {
                if let Some(href) = token.attribute("href").and_then(|href| self.resolve(href)) {
                    self.base = Url::parse(&href).ok();
                }
            }
            "br" => self.newline(),
            "hr" => {
                self.block();
                self.push("---");
                self.block();
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.block();
                let level = usize::from(name.as_bytes()[1] - b'0');
                self.push(&format!("{} ", "#".repeat(level)));
            }
            "ul" | "ol" | "menu" => {
                if self.lists.is_empty() {
                    self.block();
                } else {
                    self.line();
                }
                let start = token
                    .attribute("start")
                    .and_then(|value| value.trim().parse::<u32>().ok())
                    .unwrap_or(1);
                self.lists.push(ListFrame {
                    ordered: name == "ol",
                    counter: start,
                });
            }
            "li" => {
                self.line();
                let depth = self.lists.len().max(1);
                let marker = match self.lists.last_mut() {
                    Some(frame) if frame.ordered => {
                        let marker = format!("{}. ", frame.counter);
                        frame.counter = frame.counter.saturating_add(1);
                        marker
                    }
                    _ => "- ".to_owned(),
                };
                self.push(&format!("{}{marker}", "  ".repeat(depth - 1)));
            }
            "dt" => self.line(),
            "dd" => {
                self.line();
                self.push("  ");
            }
            "tr" => {
                self.line();
                self.cell = 0;
            }
            "td" | "th" => {
                if self.cell > 0 {
                    self.push(" | ");
                }
                self.cell += 1;
            }
            "pre" => {
                self.block();
                self.push("```\n");
                self.pre += 1;
            }
            "code" | "kbd" | "samp" if self.pre == 0 => {
                self.space();
                self.push("`");
                self.code += 1;
            }
            "a" => {
                let href = token.attribute("href").and_then(|href| self.resolve(href));
                self.space();
                self.links.push(LinkFrame {
                    href,
                    start: self.out.len(),
                });
            }
            _ if BLOCKS.contains(&name) => self.block(),
            _ => {}
        }
    }

    fn end(&mut self, name: &str) {
        if !self.skip.is_empty() {
            if let Some(position) = self.skip.iter().rposition(|open| open == name) {
                self.skip.truncate(position);
            }
            return;
        }
        if self.raw.as_deref() == Some(name) {
            self.raw = None;
            self.capture_title = false;
            return;
        }
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.block(),
            "ul" | "ol" | "menu" => {
                self.lists.pop();
                if self.lists.is_empty() {
                    self.block();
                } else {
                    self.line();
                }
            }
            "li" | "dt" | "dd" | "tr" => self.line(),
            "pre" if self.pre > 0 => {
                self.line();
                self.push("```");
                self.pre -= 1;
                self.block();
            }
            "code" | "kbd" | "samp" if self.pre == 0 && self.code > 0 => {
                self.push("`");
                self.code -= 1;
            }
            "a" => self.close_link(),
            _ if BLOCKS.contains(&name) => self.block(),
            _ => {}
        }
    }

    fn text(&mut self, raw: &str) {
        if !self.skip.is_empty() {
            return;
        }
        if self.raw.is_some() {
            if self.capture_title {
                let title = plain_text(raw, MAX_TITLE_CHARS);
                if !title.is_empty() {
                    self.title = Some(title);
                }
            }
            return;
        }
        let text = entities::decode(raw);
        if self.pre > 0 {
            self.push(&text.replace("\r\n", "\n").replace('\r', "\n"));
            return;
        }
        let starts_with_space = text.starts_with(char::is_whitespace);
        let ends_with_space = text.ends_with(char::is_whitespace);
        let mut words = text.split_whitespace().peekable();
        if words.peek().is_none() {
            if !text.is_empty() {
                self.pending_space = true;
            }
            return;
        }
        if starts_with_space {
            self.pending_space = true;
        }
        let mut first = true;
        for word in words {
            if !first {
                self.pending_space = true;
            }
            first = false;
            self.space();
            self.push(word);
        }
        self.pending_space = ends_with_space;
    }

    /// Emit a pending inter-word space unless the line is fresh.
    fn space(&mut self) {
        if self.pending_space {
            self.pending_space = false;
            if !self.out.is_empty() && !self.out.ends_with([' ', '\n']) {
                self.push(" ");
            }
        }
    }

    fn line(&mut self) {
        self.pending_space = false;
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.push("\n");
        }
    }

    fn newline(&mut self) {
        self.pending_space = false;
        if !self.out.ends_with("\n\n") {
            self.push("\n");
        }
    }

    fn block(&mut self) {
        self.pending_space = false;
        if self.out.is_empty() || self.out.ends_with("\n\n") {
            return;
        }
        if self.out.ends_with('\n') {
            self.push("\n");
        } else {
            self.push("\n\n");
        }
    }

    fn push(&mut self, text: &str) {
        if self.truncated {
            return;
        }
        let room = self.max.saturating_sub(self.out.len());
        if text.len() <= room {
            self.out.push_str(text);
            return;
        }
        let mut cut = room;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        self.out.push_str(&text[..cut]);
        self.truncated = true;
    }

    fn close_link(&mut self) {
        let Some(frame) = self.links.pop() else {
            return;
        };
        let Some(href) = frame.href else {
            return;
        };
        if self.truncated || self.pre > 0 || frame.start > self.out.len() {
            return;
        }
        let segment = &self.out[frame.start..];
        let label = segment.split_whitespace().collect::<Vec<_>>().join(" ");
        if label.is_empty() || label.contains("](") {
            return;
        }
        let lead = segment.len() - segment.trim_start().len();
        let prefix = segment[..lead].to_owned();
        let label = label.replace('[', "\\[").replace(']', "\\]");
        let link = format!("{prefix}[{label}]({href})");
        self.out.truncate(frame.start);
        self.push(&link);
    }

    /// An absolute http(s) URL for `href`, or `None` (javascript:, mailto:,
    /// fragments, credentials, and unparsable references are not links).
    fn resolve(&self, href: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty() || href.starts_with('#') {
            return None;
        }
        let url = match &self.base {
            Some(base) => base.join(href).ok()?,
            None => Url::parse(href).ok()?,
        };
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.host_str().is_none()
        {
            return None;
        }
        Some(
            url.as_str()
                .replace('(', "%28")
                .replace(')', "%29")
                .replace(' ', "%20"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(html: &str) -> String {
        extract(html, Url::parse("https://example.com/docs/page").ok().as_ref(), 1 << 20).text
    }

    #[test]
    fn documents_become_markdown_like_text() {
        let page = extract(
            r#"<!doctype html><html><head><title> Example &amp; Co </title>
            <style>body { color: red }</style><script>var x = "<p>no</p>";</script></head>
            <body><nav aria-hidden="true">menu</nav>
            <h1>Main   heading</h1>
            <p>First <b>bold</b> paragraph with a <a href="/next?a=1">relative link</a>.</p>
            <ul><li>one</li><li>two<ol><li>nested</li><li>again</li></ol></li></ul>
            <pre><code>fn main() {
    println!("hi");
}</code></pre>
            <p>Inline <code>x &lt; y</code> and <a href="javascript:alert(1)">script</a>.</p>
            <table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>
            <noscript>enable js</noscript><div hidden>secret menu</div>
            </body></html>"#,
            Url::parse("https://example.com/docs/page").ok().as_ref(),
            1 << 20,
        );
        assert_eq!(page.title.as_deref(), Some("Example & Co"));
        assert!(!page.truncated);
        let expected = "# Main heading\n\n\
            First bold paragraph with a [relative link](https://example.com/next?a=1).\n\n\
            - one\n- two\n  1. nested\n  2. again\n\n\
            ```\nfn main() {\n    println!(\"hi\");\n}\n```\n\n\
            Inline `x < y` and script.\n\n\
            A | B\n1 | 2";
        assert_eq!(page.text, expected);
        for dropped in ["color: red", "no</p>", "menu", "enable js", "secret"] {
            assert!(!page.text.contains(dropped), "{dropped}");
        }
    }

    #[test]
    fn links_resolve_against_base_and_unsafe_targets_stay_text() {
        assert_eq!(
            text(r#"<base href="https://cdn.example.org/x/"><a href="y.html">Y</a>"#),
            "[Y](https://cdn.example.org/x/y.html)"
        );
        assert_eq!(
            text(
                r##"<a href="https://u:p@example.com/">creds</a> <a href="mailto:a@b">mail</a> <a href="#top">top</a>"##
            ),
            "creds mail top"
        );
        assert_eq!(
            text(r#"<a href="https://example.com/a b(c)">see <i>this</i> [doc]</a>"#),
            "[see this \\[doc\\]](https://example.com/a%20b%28c%29)"
        );
        assert_eq!(text(r#"<a href="/x"><img alt="logo"></a>after"#), "after");
    }

    #[test]
    fn whitespace_breaks_and_entities_are_normalized() {
        assert_eq!(
            text("<p>a\n\n   b&nbsp;&nbsp;c</p><p></p><p>d<br>e<br><br><br>f</p>"),
            "a b c\n\nd\ne\n\nf"
        );
        assert_eq!(text("<span>x</span><span>y</span> <em>z</em>"), "xy z");
        assert_eq!(text("<h3>T</h3>body<hr>end"), "### T\n\nbody\n\n---\n\nend");
    }

    #[test]
    fn unclosed_dropped_elements_end_at_an_enclosing_end_tag() {
        assert_eq!(
            text("<div><div hidden><p>gone<li>also</div>kept</div>"),
            "kept"
        );
    }

    #[test]
    fn text_is_capped_at_a_character_boundary() {
        let html = format!("<p>{}</p>", "\u{e9}".repeat(100));
        let page = extract(&html, None, 51);
        assert!(page.truncated);
        assert!(page.text.len() <= 51);
        assert!(page.text.chars().all(|ch| ch == '\u{e9}'));
    }

    #[test]
    fn plain_text_strips_tags_and_truncates() {
        assert_eq!(
            plain_text("Rust <b>is</b>\n fast &amp; <i>safe</i>", 100),
            "Rust is fast & safe"
        );
        assert_eq!(plain_text("abcdef", 3), "abc\u{2026}");
    }
}
