//! Conservative, quote-aware splitting of command lines into simple commands.
//!
//! The lexer is not a shell. It over-splits on every operator any supported dialect
//! treats as a command boundary (`;`, `&`, `&&`, `|`, `||`, newlines, grouping
//! parentheses and braces, command substitution), so commands hidden inside groups,
//! subshells, script blocks, or substitutions are classified on their own.

/// Shell syntax used to interpret a command line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Dialect {
    Posix,
    Cmd,
    PowerShell,
}

/// One simple command: its words (quotes removed) and output redirection targets.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Segment {
    pub words: Vec<String>,
    pub redirects: Vec<String>,
    /// Standard input comes from the previous segment through a pipe.
    pub piped: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Lexed {
    pub segments: Vec<Segment>,
    /// Command substitutions found inside double-quoted strings.
    pub embedded: Vec<String>,
    /// A quote, substitution, or escape was left open.
    pub unbalanced: bool,
    /// The line uses command substitution (`$(...)`, backticks).
    pub substitution: bool,
}

pub(crate) fn lex(input: &str, dialect: Dialect) -> Lexed {
    let mut lexer = Lexer {
        chars: input.chars().collect(),
        pos: 0,
        dialect,
        out: Lexed::default(),
        current: Segment::default(),
        word: String::new(),
        in_word: false,
        target: Target::Argument,
        next_piped: false,
    };
    lexer.run();
    lexer.out
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Target {
    Argument,
    OutputRedirect,
    InputRedirect,
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    dialect: Dialect,
    out: Lexed,
    current: Segment,
    word: String,
    in_word: bool,
    target: Target,
    next_piped: bool,
}

impl Lexer {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn run(&mut self) {
        while let Some(ch) = self.peek(0) {
            match ch {
                '\n' | ';' => {
                    self.end_segment(false);
                    self.pos += 1;
                }
                '|' => {
                    let double = self.peek(1) == Some('|');
                    self.end_segment(!double);
                    self.pos += if double { 2 } else { 1 };
                }
                '&' => self.ampersand(),
                '>' => self.output_redirect(),
                '<' => {
                    self.end_word();
                    self.pos += 1;
                    while self.peek(0) == Some('<') {
                        self.pos += 1;
                    }
                    self.target = Target::InputRedirect;
                }
                '(' => {
                    if self.word.ends_with('$') {
                        self.word.pop();
                        self.out.substitution = true;
                    }
                    self.end_segment(false);
                    self.pos += 1;
                }
                ')' | '}' => {
                    self.end_segment(false);
                    self.pos += 1;
                }
                '{' => self.open_brace(),
                '\'' if self.dialect != Dialect::Cmd => self.single_quote(),
                '"' => self.double_quote(),
                '\\' if self.dialect == Dialect::Posix => self.escape(),
                '^' if self.dialect == Dialect::Cmd => self.escape(),
                '`' if self.dialect == Dialect::PowerShell => self.escape(),
                '`' => {
                    self.out.substitution = true;
                    self.end_segment(false);
                    self.pos += 1;
                }
                ch if ch.is_whitespace() => {
                    self.end_word();
                    self.pos += 1;
                }
                ch => {
                    self.push(ch);
                    self.pos += 1;
                }
            }
        }
        self.end_segment(false);
    }

    fn push(&mut self, ch: char) {
        self.word.push(ch);
        self.in_word = true;
    }

    fn ampersand(&mut self) {
        if self.peek(1) == Some('&') {
            self.end_segment(false);
            self.pos += 2;
        } else if self.peek(1) == Some('>') {
            // `&>file` redirects stdout and stderr.
            self.pos += 1;
            self.output_redirect();
        } else {
            self.end_segment(false);
            self.pos += 1;
        }
    }

    fn output_redirect(&mut self) {
        // A file-descriptor number written directly before `>` is not an argument.
        if self.in_word && self.word.chars().all(|ch| ch.is_ascii_digit() || ch == '*') {
            self.word.clear();
            self.in_word = false;
        }
        self.end_word();
        while self.peek(0) == Some('>') {
            self.pos += 1;
        }
        if self.peek(0) == Some('&') && self.peek(1).is_some_and(|ch| ch.is_ascii_digit()) {
            // `2>&1` duplicates a descriptor; it names no file.
            self.pos += 2;
            return;
        }
        self.target = Target::OutputRedirect;
    }

    fn open_brace(&mut self) {
        if self.peek(1) == Some('}') {
            // `{}` is a literal placeholder (`find -exec rm {} ;`), not a group.
            self.push('{');
            self.push('}');
            self.pos += 2;
            return;
        }
        if self.dialect != Dialect::Cmd && self.word.ends_with('$') {
            // `${NAME}` parameter expansion stays inside the word.
            while let Some(ch) = self.peek(0) {
                self.push(ch);
                self.pos += 1;
                if ch == '}' {
                    return;
                }
            }
            self.out.unbalanced = true;
            return;
        }
        self.end_segment(false);
        self.pos += 1;
    }

    fn escape(&mut self) {
        match self.peek(1) {
            Some('\n') => self.pos += 2,
            Some(next) => {
                self.push(next);
                self.pos += 2;
            }
            None => {
                self.out.unbalanced = true;
                self.pos += 1;
            }
        }
    }

    fn single_quote(&mut self) {
        self.in_word = true;
        self.pos += 1;
        while let Some(ch) = self.peek(0) {
            self.pos += 1;
            if ch == '\'' {
                if self.dialect == Dialect::PowerShell && self.peek(0) == Some('\'') {
                    self.word.push('\'');
                    self.pos += 1;
                    continue;
                }
                return;
            }
            self.word.push(ch);
        }
        self.out.unbalanced = true;
    }

    fn double_quote(&mut self) {
        self.in_word = true;
        self.pos += 1;
        while let Some(ch) = self.peek(0) {
            self.pos += 1;
            match ch {
                '"' => {
                    if self.dialect == Dialect::PowerShell && self.peek(0) == Some('"') {
                        self.word.push('"');
                        self.pos += 1;
                        continue;
                    }
                    return;
                }
                '\\' if self.dialect == Dialect::Posix => match self.peek(0) {
                    Some(next @ ('"' | '\\' | '$' | '`')) => {
                        self.word.push(next);
                        self.pos += 1;
                    }
                    Some('\n') => self.pos += 1,
                    _ => self.word.push('\\'),
                },
                '`' if self.dialect == Dialect::PowerShell => match self.peek(0) {
                    Some(next) => {
                        self.word.push(next);
                        self.pos += 1;
                    }
                    None => self.word.push('`'),
                },
                '`' if self.dialect == Dialect::Posix => self.capture_backticks(),
                '$' if self.dialect != Dialect::Cmd && self.peek(0) == Some('(') => {
                    self.capture_parenthesized();
                }
                ch => self.word.push(ch),
            }
        }
        self.out.unbalanced = true;
    }

    /// Capture `$( ... )` inside a double-quoted string as an embedded command.
    fn capture_parenthesized(&mut self) {
        self.out.substitution = true;
        self.pos += 1;
        let mut depth = 1_usize;
        let mut inner = String::new();
        while let Some(ch) = self.peek(0) {
            self.pos += 1;
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        self.word.push_str(&inner);
                        self.out.embedded.push(inner);
                        return;
                    }
                }
                _ => {}
            }
            inner.push(ch);
        }
        self.out.unbalanced = true;
        self.out.embedded.push(inner);
    }

    /// Capture a backtick substitution inside a POSIX double-quoted string.
    fn capture_backticks(&mut self) {
        self.out.substitution = true;
        let mut inner = String::new();
        while let Some(ch) = self.peek(0) {
            self.pos += 1;
            if ch == '`' {
                self.word.push_str(&inner);
                self.out.embedded.push(inner);
                return;
            }
            inner.push(ch);
        }
        self.out.unbalanced = true;
        self.out.embedded.push(inner);
    }

    fn end_word(&mut self) {
        if !self.in_word {
            return;
        }
        let word = std::mem::take(&mut self.word);
        self.in_word = false;
        match std::mem::replace(&mut self.target, Target::Argument) {
            Target::Argument => self.current.words.push(word),
            Target::OutputRedirect => self.current.redirects.push(word),
            Target::InputRedirect => {}
        }
    }

    /// Close the current simple command. `piped_next` marks the next one as reading
    /// this command's output.
    fn end_segment(&mut self, piped_next: bool) {
        self.end_word();
        self.target = Target::Argument;
        if self.current.words.is_empty() && self.current.redirects.is_empty() {
            self.next_piped |= piped_next;
            return;
        }
        let mut segment = std::mem::take(&mut self.current);
        segment.piped = self.next_piped;
        self.out.segments.push(segment);
        self.next_piped = piped_next;
    }
}

#[cfg(test)]
mod tests {
    use super::{Dialect, lex};

    fn words(input: &str, dialect: Dialect) -> Vec<Vec<String>> {
        lex(input, dialect)
            .segments
            .into_iter()
            .map(|segment| segment.words)
            .collect()
    }

    #[test]
    fn splits_on_every_operator_outside_quotes() {
        assert_eq!(
            words("a 1; b 2 && c | d || e & f", Dialect::Posix),
            [
                vec!["a", "1"],
                vec!["b", "2"],
                vec!["c"],
                vec!["d"],
                vec!["e"],
                vec!["f"]
            ]
        );
        assert_eq!(
            words("echo 'a; b' \"c | d\"", Dialect::Posix),
            [vec!["echo", "a; b", "c | d"]]
        );
    }

    #[test]
    fn marks_piped_segments_across_groups() {
        let lexed = lex("curl x | (sh)", Dialect::Posix);
        assert_eq!(lexed.segments.len(), 2);
        assert!(!lexed.segments[0].piped);
        assert!(lexed.segments[1].piped);
    }

    #[test]
    fn captures_redirect_targets_and_ignores_descriptor_duplication() {
        let lexed = lex("make 2>&1 >out.log 2> err.txt", Dialect::Posix);
        assert_eq!(lexed.segments[0].words, ["make"]);
        assert_eq!(lexed.segments[0].redirects, ["out.log", "err.txt"]);
    }

    #[test]
    fn extracts_substitutions_as_embedded_commands() {
        let lexed = lex("echo \"$(rm -rf /)\" `id`", Dialect::Posix);
        assert!(lexed.substitution);
        assert_eq!(lexed.embedded, ["rm -rf /"]);
        assert!(lexed.segments.iter().any(|segment| segment.words == ["id"]));
    }

    #[test]
    fn applies_dialect_escapes_and_quotes() {
        assert_eq!(words("echo a^&b", Dialect::Cmd), [vec!["echo", "a&b"]]);
        assert_eq!(
            words("echo 'x & y'", Dialect::Cmd),
            [vec!["echo", "'x"], vec!["y'"]]
        );
        assert_eq!(
            words("echo 'it''s'", Dialect::PowerShell),
            [vec!["echo", "it's"]]
        );
        assert_eq!(words("echo a\\;b", Dialect::Posix), [vec!["echo", "a;b"]]);
    }

    #[test]
    fn splits_powershell_script_blocks() {
        assert_eq!(
            words(
                "gci | % { Remove-Item $_ -Recurse -Force }",
                Dialect::PowerShell
            ),
            [
                vec!["gci"],
                vec!["%"],
                vec!["Remove-Item", "$_", "-Recurse", "-Force"]
            ]
        );
    }

    #[test]
    fn reports_unbalanced_quotes() {
        assert!(lex("echo 'open", Dialect::Posix).unbalanced);
        assert!(lex("echo \"open", Dialect::PowerShell).unbalanced);
        assert!(!lex("echo \"closed\"", Dialect::Cmd).unbalanced);
    }
}
