//! Lightweight, regex-based symbol extraction.
//!
//! One pattern list per language, matched line by line against definitions:
//! good enough to find where a function, type, or constant is declared, not a
//! parser. Multi-line signatures, macros that generate items, and nested or
//! unusual declarations can be missed; occasional false positives are possible
//! for C-family function definitions.

use std::{fmt, str::FromStr, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Longest symbol name kept.
const MAX_SYMBOL_CHARS: usize = 128;

/// What kind of definition a symbol is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Interface,
    Class,
    Type,
    Module,
    Constant,
    Macro,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Class => "class",
            Self::Type => "type",
            Self::Module => "module",
            Self::Constant => "constant",
            Self::Macro => "macro",
        }
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SymbolKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "function" => Self::Function,
            "method" => Self::Method,
            "struct" => Self::Struct,
            "enum" => Self::Enum,
            "trait" => Self::Trait,
            "interface" => Self::Interface,
            "class" => Self::Class,
            "type" => Self::Type,
            "module" => Self::Module,
            "constant" => Self::Constant,
            "macro" => Self::Macro,
            other => return Err(format!("unknown symbol kind `{other}`")),
        })
    }
}

/// A definition found in a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// 1-based line.
    pub line: u32,
}

type Patterns = Vec<(SymbolKind, Regex)>;

fn compile(patterns: &[(SymbolKind, &str)]) -> Patterns {
    patterns
        .iter()
        .filter_map(|(kind, pattern)| Regex::new(pattern).ok().map(|regex| (*kind, regex)))
        .collect()
}

const RUST_VIS: &str = r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?";

static RUST: LazyLock<Patterns> = LazyLock::new(|| {
    let item = |body: &str| format!("{RUST_VIS}{body}");
    let owned = [
        (
            SymbolKind::Function,
            item(
                r#"(?:(?:default|async|const|unsafe|extern(?:\s+"[^"]*")?)\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)"#,
            ),
        ),
        (
            SymbolKind::Struct,
            item(r"(?:union\s+|struct\s+)([A-Za-z_][A-Za-z0-9_]*)"),
        ),
        (SymbolKind::Enum, item(r"enum\s+([A-Za-z_][A-Za-z0-9_]*)")),
        (
            SymbolKind::Trait,
            item(r"(?:unsafe\s+)?(?:auto\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)"),
        ),
        (SymbolKind::Type, item(r"type\s+([A-Za-z_][A-Za-z0-9_]*)")),
        (SymbolKind::Module, item(r"mod\s+([A-Za-z_][A-Za-z0-9_]*)")),
        (
            SymbolKind::Constant,
            item(r"(?:const|static)\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:"),
        ),
        (
            SymbolKind::Macro,
            r"^\s*macro_rules!\s*([A-Za-z_][A-Za-z0-9_]*)".to_owned(),
        ),
    ];
    owned
        .iter()
        .filter_map(|(kind, pattern)| Regex::new(pattern).ok().map(|regex| (*kind, regex)))
        .collect()
});

static TS_JS: LazyLock<Patterns> = LazyLock::new(|| {
    compile(&[
        (
            SymbolKind::Function,
            r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)",
        ),
        (
            SymbolKind::Class,
            r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)",
        ),
        (
            SymbolKind::Interface,
            r"^\s*(?:export\s+)?(?:declare\s+)?interface\s+([A-Za-z_$][\w$]*)",
        ),
        (
            SymbolKind::Type,
            r"^\s*(?:export\s+)?(?:declare\s+)?type\s+([A-Za-z_$][\w$]*)\s*(?:<[^=]*>)?\s*=",
        ),
        (
            SymbolKind::Enum,
            r"^\s*(?:export\s+)?(?:declare\s+)?(?:const\s+)?enum\s+([A-Za-z_$][\w$]*)",
        ),
        (
            SymbolKind::Function,
            r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s+)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>",
        ),
        (
            SymbolKind::Function,
            r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s+)?function\b",
        ),
        (
            SymbolKind::Constant,
            r"^\s*export\s+const\s+([A-Z_][A-Z0-9_]*)\s*(?::[^=]+)?=",
        ),
    ])
});

static PYTHON: LazyLock<Patterns> = LazyLock::new(|| {
    compile(&[
        (
            SymbolKind::Function,
            r"^(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)",
        ),
        (
            SymbolKind::Method,
            r"^\s+(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)",
        ),
        (SymbolKind::Class, r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)"),
        (
            SymbolKind::Constant,
            r"^([A-Z][A-Z0-9_]+)\s*(?::[^=]+)?=[^=]",
        ),
    ])
});

static GO: LazyLock<Patterns> = LazyLock::new(|| {
    compile(&[
        (SymbolKind::Method, r"^func\s+\([^)]*\)\s*([A-Za-z_]\w*)"),
        (SymbolKind::Function, r"^func\s+([A-Za-z_]\w*)"),
        (
            SymbolKind::Struct,
            r"^type\s+([A-Za-z_]\w*)(?:\[[^\]]*\])?\s+struct\b",
        ),
        (
            SymbolKind::Interface,
            r"^type\s+([A-Za-z_]\w*)(?:\[[^\]]*\])?\s+interface\b",
        ),
        (SymbolKind::Type, r"^type\s+([A-Za-z_]\w*)\b"),
        (SymbolKind::Constant, r"^const\s+([A-Za-z_]\w*)\b"),
    ])
});

static C_FAMILY: LazyLock<Patterns> = LazyLock::new(|| {
    compile(&[
        (
            SymbolKind::Class,
            r"^\s*(?:template\s*<[^>]*>\s*)?class\s+(?:[A-Z_]+\s+)?([A-Za-z_]\w*)\s*(?:final\s*)?(?:[:{]|$)",
        ),
        (
            SymbolKind::Struct,
            r"^\s*(?:typedef\s+)?(?:struct|union)\s+([A-Za-z_]\w*)\s*(?:[:{]|$)",
        ),
        (
            SymbolKind::Enum,
            r"^\s*(?:typedef\s+)?enum\s+(?:class\s+|struct\s+)?([A-Za-z_]\w*)\s*(?:[:{]|$)",
        ),
        (
            SymbolKind::Module,
            r"^\s*namespace\s+([A-Za-z_]\w*)\s*\{?\s*$",
        ),
        (SymbolKind::Macro, r"^\s*#\s*define\s+([A-Za-z_]\w*)"),
        (
            SymbolKind::Function,
            r"^(?:[A-Za-z_][\w:<>,\*&\s]*?[\s\*&])?([A-Za-z_][\w]*(?:::[A-Za-z_~]\w*)*)\s*\([^;]*\)\s*(?:const\s*)?(?:noexcept\s*)?(?:override\s*)?\{?\s*$",
        ),
    ])
});

const JVM_MODIFIERS: &str = r"(?:(?:public|private|protected|internal|static|abstract|final|sealed|partial|readonly|unsafe|override|virtual|async|synchronized|native|extern|new)\s+)";

static JAVA_CSHARP: LazyLock<Patterns> = LazyLock::new(|| {
    let types = format!(
        r"^\s*(?:\[[^\]]*\]\s*)?(?:@\w+\s+)*{JVM_MODIFIERS}*(?:class|interface|enum|record|struct)\s+([A-Za-z_]\w*)"
    );
    let methods = format!(
        r"^\s*(?:@\w+\s+)*{JVM_MODIFIERS}+(?:<[^>]*>\s*)?[\w<>\[\],\.\?]+(?:\s*<[^>]*>)?\s+([A-Za-z_]\w*)\s*\("
    );
    [(SymbolKind::Class, types), (SymbolKind::Method, methods)]
        .iter()
        .filter_map(|(kind, pattern)| Regex::new(pattern).ok().map(|regex| (*kind, regex)))
        .collect()
});

/// Words that the C-family function pattern can mistake for names.
const NOT_NAMES: [&str; 12] = [
    "if", "for", "while", "switch", "return", "sizeof", "catch", "else", "do", "case", "new",
    "delete",
];

fn patterns_for(language: &str) -> Option<&'static Patterns> {
    Some(match language {
        "rust" => &RUST,
        "typescript" | "javascript" => &TS_JS,
        "python" => &PYTHON,
        "go" => &GO,
        "c" | "cpp" => &C_FAMILY,
        "java" | "csharp" => &JAVA_CSHARP,
        _ => return None,
    })
}

/// Extract definitions from `text` whose first line is line `first_line`.
pub(crate) fn extract(language: Option<&str>, text: &str, first_line: u32, out: &mut Vec<Symbol>) {
    let Some(patterns) = language.and_then(patterns_for) else {
        return;
    };
    for (offset, line) in text.lines().enumerate() {
        if line.len() > 512 {
            continue;
        }
        for (kind, regex) in patterns {
            let Some(name) = regex.captures(line).and_then(|captures| captures.get(1)) else {
                continue;
            };
            let name = name.as_str();
            let base = name.rsplit("::").next().unwrap_or(name);
            if name.chars().count() > MAX_SYMBOL_CHARS || NOT_NAMES.contains(&base) {
                continue;
            }
            out.push(Symbol {
                name: name.to_owned(),
                kind: *kind,
                line: first_line + offset as u32,
            });
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(language: &str, text: &str) -> Vec<(String, SymbolKind, u32)> {
        let mut out = Vec::new();
        extract(Some(language), text, 1, &mut out);
        out.into_iter()
            .map(|symbol| (symbol.name, symbol.kind, symbol.line))
            .collect()
    }

    #[test]
    fn every_pattern_compiles() {
        assert_eq!(RUST.len(), 8);
        assert_eq!(TS_JS.len(), 8);
        assert_eq!(PYTHON.len(), 4);
        assert_eq!(GO.len(), 6);
        assert_eq!(C_FAMILY.len(), 6);
        assert_eq!(JAVA_CSHARP.len(), 2);
    }

    #[test]
    fn rust_definitions() {
        let text = "pub fn parse_config() {}\n    pub(crate) async fn load() {}\nstruct Index;\npub enum Mode {}\npub trait Store {}\ntype Alias = u8;\nmod tests {}\npub const MAX_FILES: usize = 1;\nmacro_rules! make {\nlet x = fn_call();\n";
        let found = names("rust", text);
        assert_eq!(
            found,
            [
                ("parse_config".into(), SymbolKind::Function, 1),
                ("load".into(), SymbolKind::Function, 2),
                ("Index".into(), SymbolKind::Struct, 3),
                ("Mode".into(), SymbolKind::Enum, 4),
                ("Store".into(), SymbolKind::Trait, 5),
                ("Alias".into(), SymbolKind::Type, 6),
                ("tests".into(), SymbolKind::Module, 7),
                ("MAX_FILES".into(), SymbolKind::Constant, 8),
                ("make".into(), SymbolKind::Macro, 9),
            ]
        );
    }

    #[test]
    fn typescript_python_go_definitions() {
        let ts = "export async function fetchUser(id) {\nexport class Store {\ninterface Props {\nexport type Id = string;\nexport const handler = async (req) => {\nconst value = 3;\n";
        let found: Vec<String> = names("typescript", ts).into_iter().map(|s| s.0).collect();
        assert_eq!(found, ["fetchUser", "Store", "Props", "Id", "handler"]);

        let py = "def main():\nclass Server:\n    def handle(self):\nTIMEOUT = 30\n";
        let found = names("python", py);
        assert_eq!(found[0], ("main".into(), SymbolKind::Function, 1));
        assert_eq!(found[1], ("Server".into(), SymbolKind::Class, 2));
        assert_eq!(found[2], ("handle".into(), SymbolKind::Method, 3));
        assert_eq!(found[3], ("TIMEOUT".into(), SymbolKind::Constant, 4));

        let go = "func (s *Server) Serve() error {\nfunc main() {\ntype Config struct {\ntype Reader interface {\n";
        let found: Vec<(String, SymbolKind, u32)> = names("go", go);
        assert_eq!(found[0].0, "Serve");
        assert_eq!(found[0].1, SymbolKind::Method);
        assert_eq!(found[1].0, "main");
        assert_eq!(found[2], ("Config".into(), SymbolKind::Struct, 3));
        assert_eq!(found[3], ("Reader".into(), SymbolKind::Interface, 4));
    }

    #[test]
    fn c_family_and_jvm_definitions() {
        let c = "int parse_args(int argc, char **argv) {\nstruct options {\n#define MAX_LEN 10\n    if (x) {\nvoid Widget::draw() const {\nnamespace app {\n";
        let found: Vec<String> = names("cpp", c).into_iter().map(|s| s.0).collect();
        assert_eq!(
            found,
            ["parse_args", "options", "MAX_LEN", "Widget::draw", "app"]
        );
        let java = "public class Main {\n    public static void main(String[] args) {\n    private final int count;\n";
        let found = names("java", java);
        assert_eq!(found[0], ("Main".into(), SymbolKind::Class, 1));
        assert_eq!(found[1], ("main".into(), SymbolKind::Method, 2));
        assert_eq!(found.len(), 2);
        let cs = "public sealed class Repo\n    public async Task<int> CountAsync(string name)\n";
        let found: Vec<String> = names("csharp", cs).into_iter().map(|s| s.0).collect();
        assert_eq!(found, ["Repo", "CountAsync"]);
    }

    #[test]
    fn unknown_languages_have_no_symbols() {
        let mut out = Vec::new();
        extract(Some("markdown"), "# fn main()", 1, &mut out);
        extract(None, "fn main()", 1, &mut out);
        assert!(out.is_empty());
        assert_eq!("struct".parse::<SymbolKind>(), Ok(SymbolKind::Struct));
        assert!("nope".parse::<SymbolKind>().is_err());
    }
}
