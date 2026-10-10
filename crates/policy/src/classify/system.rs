//! Host power, services, registry, credentials, decoders, and the fallback tables
//! for read-only, file-writing, developer, and unknown programs.

use super::{
    programs::{Ctx, has_flag},
    shell::ShellRisk,
};
use crate::error::truncate;

/// Programs no family module recognised.
pub(super) fn fallback(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    if LOW_RISK.contains(&program) {
        low_risk(program, args, ctx);
    } else if WRITERS.contains(&program) {
        ctx.acc.writes();
        ctx.raise(
            ShellRisk::Medium,
            "creates, copies, moves, or rewrites files",
        );
    } else if DEV_TOOLS.contains(&program) {
        ctx.raise(ShellRisk::Medium, "runs a build, test, or developer tool");
    } else {
        ctx.raise(
            ShellRisk::Medium,
            format!("unrecognized program `{}`", truncate(program)),
        );
    }
}

/// Host power, services, accounts, registry, firewall, scheduling, and credentials.
pub(super) fn classify(program: &str, args: &[String], ctx: &mut Ctx<'_>) -> bool {
    match program {
        "shutdown" | "reboot" | "halt" | "poweroff" | "stop-computer" | "restart-computer"
        | "logoff" | "init" | "telinit" => {
            ctx.raise(
                ShellRisk::High,
                "shuts down, restarts, or logs off the host",
            );
        }
        "systemctl" => {
            let power = [
                "poweroff",
                "reboot",
                "halt",
                "suspend",
                "hibernate",
                "kexec",
            ];
            if args.iter().any(|arg| power.contains(&arg.as_str())) {
                ctx.raise(
                    ShellRisk::High,
                    "shuts down, restarts, or logs off the host",
                );
            } else if has_flag(args, &["status", "list-units", "is-active", "show", "cat"]) {
                ctx.raise(ShellRisk::Low, "inspects system services");
            } else {
                ctx.raise(ShellRisk::High, "changes system services");
            }
        }
        "kill" | "killall" | "pkill" | "taskkill" | "stop-process" | "spps" | "xkill" => {
            if has_flag(args, &["-9", "/f", "-force", "-kill", "-sigkill"]) {
                ctx.raise(ShellRisk::High, "forcibly terminates processes");
            } else {
                ctx.raise(ShellRisk::Medium, "terminates processes");
            }
        }
        "reg" => {
            if args
                .first()
                .is_some_and(|sub| sub.eq_ignore_ascii_case("query"))
            {
                ctx.raise(ShellRisk::Low, "reads the Windows registry");
            } else {
                ctx.acc.writes();
                ctx.raise(ShellRisk::High, "modifies the Windows registry");
            }
        }
        "set-itemproperty" | "new-itemproperty" | "remove-itemproperty" | "sp" => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::High, "modifies item properties or the registry");
        }
        "service"
        | "sc"
        | "launchctl"
        | "crontab"
        | "schtasks"
        | "at"
        | "netsh"
        | "iptables"
        | "ip6tables"
        | "ufw"
        | "nft"
        | "pfctl"
        | "set-executionpolicy"
        | "bcdedit"
        | "mount"
        | "umount"
        | "modprobe"
        | "insmod"
        | "rmmod"
        | "sysctl"
        | "useradd"
        | "userdel"
        | "usermod"
        | "passwd"
        | "net"
        | "wevtutil"
        | "csrutil"
        | "spctl"
        | "setenforce"
        | "new-service"
        | "set-service"
        | "register-scheduledtask"
        | "auditpol" => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::High, "changes host system configuration");
        }
        "setx" => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::Medium, "persists environment variables");
        }
        "printenv" => ctx.raise(
            ShellRisk::Medium,
            "prints environment variables, which may contain secrets",
        ),
        "set" if args.is_empty() => ctx.raise(
            ShellRisk::Medium,
            "prints environment variables, which may contain secrets",
        ),
        "security" | "cmdkey" | "vaultcmd" | "keyctl" | "secret-tool" => {
            ctx.acc.credentials = true;
            ctx.raise(ShellRisk::High, "accesses the system credential store");
        }
        "mimikatz" | "lazagne" => {
            ctx.acc.credentials = true;
            ctx.acc.imply(crate::Action::CredentialExport);
            ctx.raise(ShellRisk::Destructive, "extracts stored credentials");
        }
        "gpg" if args.iter().any(|arg| arg.starts_with("--export-secret")) => {
            ctx.acc.imply(crate::Action::CredentialExport);
            ctx.raise(ShellRisk::Destructive, "exports private keys");
        }
        "base64" | "xxd" | "openssl" | "certutil" if decodes(program, args) => {
            ctx.acc.decoder = true;
            ctx.facts.decoder = true;
            ctx.raise(ShellRisk::Low, "decodes encoded data");
        }
        _ => return false,
    }
    true
}

fn decodes(program: &str, args: &[String]) -> bool {
    match program {
        "base64" => has_flag(args, &["-d", "--decode", "-D"]),
        "xxd" => has_flag(args, &["-r", "-revert"]),
        "openssl" => has_flag(args, &["base64", "-base64", "-a"]) && has_flag(args, &["-d"]),
        "certutil" => has_flag(args, &["-decode", "/decode", "-decodehex", "/decodehex"]),
        _ => false,
    }
}

fn low_risk(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    let env_drive = args.iter().any(|arg| {
        let lower = arg.to_ascii_lowercase();
        lower == "env:" || lower.starts_with("env:\\") || lower.starts_with("env:/")
    });
    if env_drive {
        ctx.raise(
            ShellRisk::Medium,
            "prints environment variables, which may contain secrets",
        );
        return;
    }
    match program {
        "echo" | "printf" | "write-output" | "write-host" | "echo." => {
            ctx.facts.echo_text = Some(args.join(" "));
        }
        "sed"
            if args
                .iter()
                .any(|arg| arg.starts_with("-i") || arg == "--in-place") =>
        {
            ctx.acc.writes();
            ctx.raise(ShellRisk::Medium, "edits files in place");
            return;
        }
        "awk" | "gawk" | "nawk" if args.iter().any(|arg| arg.contains("system(")) => {
            ctx.raise(ShellRisk::Medium, "awk program runs commands");
            return;
        }
        _ => {}
    }
    ctx.raise(ShellRisk::Low, "read-only command");
}

/// Read-only inspection and output commands.
const LOW_RISK: &[&str] = &[
    "ls",
    "dir",
    "cat",
    "type",
    "head",
    "tail",
    "less",
    "more",
    "echo",
    "echo.",
    "printf",
    "pwd",
    "cd",
    "chdir",
    "pushd",
    "popd",
    "wc",
    "sort",
    "uniq",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "ag",
    "ack",
    "findstr",
    "select-string",
    "sls",
    "which",
    "where",
    "whereis",
    "whoami",
    "date",
    "stat",
    "file",
    "du",
    "df",
    "tree",
    "true",
    "false",
    "test",
    "[",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "sleep",
    "hostname",
    "uname",
    "id",
    "cut",
    "tr",
    "diff",
    "cmp",
    "comm",
    "fc",
    "jq",
    "yq",
    "nl",
    "column",
    "fold",
    "expand",
    "seq",
    "sed",
    "awk",
    "gawk",
    "nawk",
    "get-content",
    "gc",
    "get-childitem",
    "gci",
    "get-item",
    "gi",
    "get-location",
    "gl",
    "set-location",
    "sl",
    "write-output",
    "write-host",
    "select-object",
    "select",
    "where-object",
    "?",
    "foreach-object",
    "%",
    "sort-object",
    "measure-object",
    "format-table",
    "ft",
    "format-list",
    "fl",
    "format-wide",
    "fw",
    "out-string",
    "out-host",
    "test-path",
    "resolve-path",
    "split-path",
    "join-path",
    "get-process",
    "ps",
    "gps",
    "get-service",
    "get-command",
    "gcm",
    "get-help",
    "get-date",
    "get-filehash",
    "sha256sum",
    "sha1sum",
    "md5sum",
    "shasum",
    "ver",
    "vol",
    "cls",
    "clear",
    "history",
    "man",
    "help",
    "exit",
    "lsof",
    "top",
    "htop",
    "uptime",
    "free",
    "env-info",
    "get-member",
    "gm",
    "compare-object",
    "group-object",
    "convertto-json",
    "convertfrom-json",
    "get-acl",
    "base64",
    "xxd",
];

/// Commands that create or rewrite files.
const WRITERS: &[&str] = &[
    "mkdir",
    "md",
    "touch",
    "new-item",
    "ni",
    "cp",
    "copy",
    "copy-item",
    "cpi",
    "mv",
    "move",
    "move-item",
    "mi",
    "ren",
    "rename",
    "rename-item",
    "rni",
    "ln",
    "mklink",
    "set-content",
    "add-content",
    "ac",
    "out-file",
    "xcopy",
    "install",
    "patch",
    "tar",
    "unzip",
    "zip",
    "7z",
    "gzip",
    "gunzip",
    "expand-archive",
    "compress-archive",
    "tee",
    "tee-object",
    "clear-content",
    "clc",
    "attrib",
    "split",
    "csplit",
];

/// Build, test, lint, and compiler tools that run project code or write build output.
const DEV_TOOLS: &[&str] = &[
    "make", "cmake", "ninja", "meson", "msbuild", "mvn", "gradle", "gradlew", "ant", "tsc",
    "eslint", "prettier", "pytest", "jest", "vitest", "mocha", "rustc", "gcc", "g++", "cc",
    "clang", "clang++", "cl", "javac", "java", "swift", "swiftc", "zig", "just", "task", "tox",
    "nox", "ruff", "black", "mypy", "rustfmt", "turbo", "nx", "vite", "webpack", "code", "vim",
    "nvim", "nano", "notepad",
];
