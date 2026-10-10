//! Quoting an argument vector into one command line, for classification and display.

use aidex_policy::{ShellPlatform, redact_command};

fn is_plain(arg: &str) -> bool {
    !arg.is_empty()
        && arg.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(ch, '_' | '-' | '.' | '/' | '=' | ',' | '+' | '@')
        })
}

fn quote(arg: &str, platform: ShellPlatform) -> String {
    if is_plain(arg) {
        return arg.to_owned();
    }
    match platform {
        ShellPlatform::Unix => format!("'{}'", arg.replace('\'', r"'\''")),
        ShellPlatform::Windows => format!("\"{}\"", arg.replace('"', "\\\"")),
    }
}

/// `program` and `args` as one command line for `platform`. Arguments that are not
/// plain words are quoted, so the classifier sees metacharacters as literal text
/// (argument vectors are never interpreted by a shell).
pub(crate) fn command_line(program: &str, args: &[String], platform: ShellPlatform) -> String {
    let mut line = quote(program, platform);
    for arg in args {
        line.push(' ');
        line.push_str(&quote(arg, platform));
    }
    line
}

/// The command line with likely credentials removed.
pub(crate) fn display_command(program: &str, args: &[String]) -> String {
    redact_command(&command_line(program, args, ShellPlatform::current()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_words_are_unquoted_and_others_quoted() {
        let args = vec!["status".to_owned(), "a b".to_owned(), "it's".to_owned()];
        assert_eq!(
            command_line("git", &args, ShellPlatform::Unix),
            r"git status 'a b' 'it'\''s'"
        );
        assert_eq!(
            command_line("git", &["x\"y".to_owned()], ShellPlatform::Windows),
            "git \"x\\\"y\""
        );
        assert_eq!(
            command_line("echo", &[String::new()], ShellPlatform::Unix),
            "echo ''"
        );
    }

    #[test]
    fn metacharacters_stay_inside_quotes() {
        let line = command_line("echo", &["a; rm -rf /".to_owned()], ShellPlatform::Unix);
        assert_eq!(line, "echo 'a; rm -rf /'");
    }
}
