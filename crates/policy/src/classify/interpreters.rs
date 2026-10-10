//! Programs that run other code: privilege wrappers, nested shells, `eval`-style
//! evaluators, and script interpreters.

use super::{
    lexer::Dialect,
    packages,
    programs::{Ctx, classify_words, has_flag, is_assignment, operands, skip_options},
    shell::ShellRisk,
};

/// Programs that run another command line.
pub(super) fn wrappers(program: &str, args: &[String], ctx: &mut Ctx<'_>) -> bool {
    match program {
        "sudo" | "doas" | "gsudo" | "pkexec" | "runas" | "su" => {
            ctx.raise(ShellRisk::High, "runs with elevated privileges");
            let rest = skip_options(args, &["-u", "-g", "-C", "-h", "-p", "-U", "-r", "-t"]);
            let rest = if program == "runas" {
                let skip = rest.iter().take_while(|arg| arg.starts_with('/')).count();
                &rest[skip..]
            } else {
                rest
            };
            classify_words(rest, ctx);
        }
        "env" => {
            let rest = skip_options(args, &["-u", "-C", "-S"]);
            let skip = rest.iter().take_while(|arg| is_assignment(arg)).count();
            if rest.len() == skip {
                ctx.raise(
                    ShellRisk::Medium,
                    "prints environment variables, which may contain secrets",
                );
            } else {
                classify_words(&rest[skip..], ctx);
            }
        }
        "nohup" | "time" | "nice" | "ionice" | "stdbuf" | "exec" | "builtin" | "caffeinate"
        | "chrt" | "taskset" | "unbuffer" | "watch" | "strace" | "ltrace" | "command" => {
            classify_words(skip_options(args, &["-n", "-c", "-p", "-o", "-s"]), ctx);
        }
        "timeout" => {
            if args.first().is_some_and(|arg| arg.starts_with('/')) {
                ctx.raise(ShellRisk::Low, "waits for a timeout");
            } else {
                let rest = skip_options(args, &["-s", "-k", "--signal", "--kill-after"]);
                classify_words(rest.get(1..).unwrap_or_default(), ctx);
            }
        }
        "xargs" => {
            ctx.raise(ShellRisk::Medium, "runs a command for each input item");
            let rest = skip_options(args, &["-n", "-I", "-P", "-L", "-d", "-s", "-E", "-a"]);
            classify_words(rest, ctx);
        }
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "ash" | "csh" | "tcsh" => {
            posix_shell(args, ctx);
        }
        "cmd" => {
            match args
                .iter()
                .position(|arg| arg.eq_ignore_ascii_case("/c") || arg.eq_ignore_ascii_case("/k"))
            {
                Some(index) => ctx.nested(&args[index + 1..].join(" "), Dialect::Cmd),
                None => {
                    ctx.facts.stdin_executor = true;
                    ctx.raise(ShellRisk::Medium, "starts a command interpreter");
                }
            }
        }
        "powershell" | "pwsh" | "powershell_ise" => powershell(args, ctx),
        "eval" => ctx.evaluated(
            &args.join(" "),
            ctx.dialect,
            "evaluates dynamically constructed code",
        ),
        "iex" | "invoke-expression" => {
            if args.is_empty() {
                ctx.facts.stdin_executor = true;
                ctx.acc.evaluator = true;
                ctx.raise(ShellRisk::Medium, "evaluates piped input as code");
            } else {
                ctx.evaluated(
                    &args.join(" "),
                    Dialect::PowerShell,
                    "evaluates dynamically constructed code",
                );
            }
        }
        "source" | "." => ctx.raise(ShellRisk::Medium, "runs a script in the current shell"),
        "start-process" | "saps" | "start" => {
            ctx.raise(ShellRisk::Medium, "starts a separate process");
            if has_flag(args, &["runas"]) {
                ctx.raise(ShellRisk::High, "runs with elevated privileges");
            }
            let program: Vec<String> = args
                .iter()
                .filter(|arg| !arg.starts_with(['-', '/']))
                .cloned()
                .collect();
            classify_words(&program, ctx);
        }
        "invoke-command" | "icm" => {
            if args
                .iter()
                .any(|arg| arg.to_ascii_lowercase().starts_with("-computer"))
            {
                ctx.acc.network();
                ctx.raise(ShellRisk::High, "runs commands on another computer");
            } else {
                ctx.raise(ShellRisk::Medium, "runs a script block");
            }
        }
        _ => return false,
    }
    true
}

fn posix_shell(args: &[String], ctx: &mut Ctx<'_>) {
    let inline = args
        .iter()
        .position(|arg| arg.starts_with('-') && !arg.starts_with("--") && arg.contains('c'));
    match inline {
        Some(index) => match args.get(index + 1) {
            Some(script) => ctx.nested(script, Dialect::Posix),
            None => ctx.raise(ShellRisk::High, "shell `-c` without a command"),
        },
        None if operands(args).next().is_some() => {
            ctx.raise(ShellRisk::Medium, "runs a shell script");
        }
        None => {
            ctx.facts.stdin_executor = true;
            ctx.raise(ShellRisk::Medium, "starts a shell");
        }
    }
}

fn powershell(args: &[String], ctx: &mut Ctx<'_>) {
    let lower: Vec<String> = args.iter().map(|arg| arg.to_ascii_lowercase()).collect();
    if lower
        .windows(2)
        .any(|pair| pair[0].starts_with("-ex") && pair[1] == "bypass")
    {
        ctx.raise(ShellRisk::High, "bypasses the PowerShell execution policy");
    }
    let encoded = lower.iter().any(|arg| {
        arg == "-e"
            || arg == "-ec"
            || (arg.len() >= 3 && "-encodedcommand".starts_with(arg.as_str()))
    });
    if encoded {
        ctx.acc.decoder = true;
        ctx.acc.evaluator = true;
        ctx.raise(
            ShellRisk::High,
            "runs an encoded (obfuscated) PowerShell command",
        );
        return;
    }
    let command = lower
        .iter()
        .position(|arg| arg.len() >= 2 && "-command".starts_with(arg.as_str()));
    if let Some(index) = command {
        ctx.nested(&args[index + 1..].join(" "), Dialect::PowerShell);
        return;
    }
    if lower
        .iter()
        .any(|arg| arg.len() >= 2 && "-file".starts_with(arg.as_str()))
    {
        ctx.raise(ShellRisk::Medium, "runs a PowerShell script");
        return;
    }
    // Windows PowerShell treats leading positional text as a command.
    const WITH_VALUE: [&str; 8] = [
        "-executionpolicy",
        "-windowstyle",
        "-outputformat",
        "-inputformat",
        "-configurationname",
        "-workingdirectory",
        "-version",
        "-settingsfile",
    ];
    let mut positional = Vec::new();
    let mut skip_value = false;
    for (arg, lowered) in args.iter().zip(&lower) {
        if skip_value {
            skip_value = false;
        } else if lowered.starts_with('-') {
            skip_value = lowered.len() >= 3
                && WITH_VALUE
                    .iter()
                    .any(|name| name.starts_with(lowered.as_str()));
        } else {
            positional.push(arg.as_str());
        }
    }
    if positional.is_empty() {
        ctx.facts.stdin_executor = true;
        ctx.raise(ShellRisk::Medium, "starts PowerShell");
    } else {
        ctx.nested(&positional.join(" "), Dialect::PowerShell);
    }
}

/// Script interpreters: inline code, script files, and stdin execution.
pub(super) fn interpreters(program: &str, args: &[String], ctx: &mut Ctx<'_>) -> bool {
    const INTERPRETERS: [&str; 18] = [
        "python",
        "python3",
        "py",
        "node",
        "deno",
        "perl",
        "ruby",
        "php",
        "lua",
        "rscript",
        "osascript",
        "wscript",
        "cscript",
        "mshta",
        "rundll32",
        "regsvr32",
        "tclsh",
        "groovy",
    ];
    if !INTERPRETERS.contains(&program) && !program.starts_with("python3.") {
        return false;
    }
    if matches!(program, "mshta" | "rundll32" | "regsvr32") {
        ctx.raise(ShellRisk::High, "runs code through a system binary proxy");
        return true;
    }
    if let Some(index) = args.iter().position(|arg| arg == "-m") {
        let module = args.get(index + 1).map(String::as_str).unwrap_or_default();
        let rest = args.get(index + 2..).unwrap_or_default();
        if module == "pip" {
            packages::pip_like("pip", rest, ctx);
        } else if module == "http.server" {
            ctx.acc.network();
            ctx.raise(ShellRisk::Medium, "serves files over HTTP");
        } else {
            ctx.raise(ShellRisk::Medium, "runs a script module");
        }
        return true;
    }
    let inline = args.iter().position(|arg| {
        matches!(
            arg.as_str(),
            "-c" | "-e" | "-E" | "--eval" | "-p" | "--print" | "-r" | "--command"
        )
    });
    if let Some(index) = inline {
        ctx.acc.evaluator = true;
        let code = args
            .get(index + 1)
            .map(|code| code.to_ascii_lowercase())
            .unwrap_or_default();
        ctx.raise(ShellRisk::Medium, "runs inline code");
        if code.contains("base64") || code.contains("b64decode") || code.contains("fromcharcode") {
            ctx.acc.decoder = true;
        }
        if [
            "system(",
            "subprocess",
            "child_process",
            "os.remove",
            "rmtree",
            "unlink",
        ]
        .iter()
        .any(|marker| code.contains(marker))
        {
            ctx.raise(
                ShellRisk::High,
                "inline code runs commands or deletes files",
            );
        }
        return true;
    }
    if operands(args).next().is_some() {
        ctx.raise(ShellRisk::Medium, "runs a script");
    } else {
        ctx.facts.stdin_executor = true;
        ctx.acc.evaluator = true;
        ctx.raise(ShellRisk::Medium, "starts an interpreter");
    }
    true
}
