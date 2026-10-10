//! Package-manager classification: installs imply `package.install`.

use super::{
    programs::{Ctx, has_flag},
    shell::ShellRisk,
};
use crate::Action;

pub(super) fn classify(program: &str, args: &[String], ctx: &mut Ctx<'_>) -> bool {
    match program {
        "npm" | "pnpm" | "yarn" | "bun" | "cnpm" => node(program, args, ctx),
        "npx" | "bunx" | "pnpx" | "uvx" => {
            install(ctx, ShellRisk::Medium, "downloads and runs a package")
        }
        "pip" | "pip3" | "pipx" | "poetry" | "pdm" | "conda" | "mamba" | "micromamba" => {
            pip_like(program, args, ctx);
        }
        "uv" => match first_operand(args).as_deref() {
            Some("pip") => {
                let index = args
                    .iter()
                    .position(|arg| arg == "pip")
                    .map_or(0, |index| index + 1);
                pip_like("pip", &args[index..], ctx);
            }
            Some("add" | "sync" | "lock") => {
                install(ctx, ShellRisk::Medium, "installs project packages")
            }
            Some("tool") if has_flag(args, &["install"]) => {
                install(ctx, ShellRisk::Medium, "installs a tool package");
            }
            Some("venv") => {
                ctx.acc.writes();
                ctx.raise(ShellRisk::Medium, "creates a virtual environment");
            }
            _ => ctx.raise(ShellRisk::Medium, "runs a project command"),
        },
        "cargo" => cargo(args, ctx),
        "gem" | "composer" | "nuget" => match first_operand(args).as_deref() {
            Some("install" | "require" | "update" | "restore" | "add") => {
                install(ctx, ShellRisk::Medium, "installs packages");
            }
            Some("list" | "search" | "show" | "info") => {
                ctx.raise(ShellRisk::Low, "inspects packages");
            }
            _ => ctx.raise(ShellRisk::Medium, "runs a package-manager command"),
        },
        "go" => match first_operand(args).as_deref() {
            Some("install" | "get") => install(ctx, ShellRisk::Medium, "installs Go packages"),
            Some("mod") if has_flag(args, &["download", "tidy", "vendor"]) => {
                install(ctx, ShellRisk::Medium, "downloads Go modules");
            }
            Some("version" | "env" | "list" | "doc") => {
                ctx.raise(ShellRisk::Low, "inspects the Go toolchain");
            }
            _ => ctx.raise(ShellRisk::Medium, "runs a build, test, or developer tool"),
        },
        "dotnet" => {
            if has_flag(args, &["add", "restore"])
                || (has_flag(args, &["tool"]) && has_flag(args, &["install"]))
            {
                install(ctx, ShellRisk::Medium, "installs .NET packages");
            } else {
                ctx.raise(ShellRisk::Medium, "runs a build, test, or developer tool");
            }
        }
        "rustup" => {
            if has_flag(args, &["install", "update", "add", "toolchain", "self"]) {
                install(ctx, ShellRisk::Medium, "installs Rust toolchain components");
            } else {
                ctx.raise(ShellRisk::Low, "inspects the Rust toolchain");
            }
        }
        "apt" | "apt-get" | "aptitude" | "yum" | "dnf" | "pacman" | "zypper" | "apk" | "emerge"
        | "port" | "choco" | "winget" | "snap" | "flatpak" | "nix-env" | "pkg" | "brew"
        | "scoop" => system(program, args, ctx),
        "install-module" | "install-package" | "install-script" | "save-module"
        | "update-module" => {
            install(ctx, ShellRisk::Medium, "installs PowerShell modules");
        }
        "msiexec" | "add-appxpackage" | "installer" => {
            install(ctx, ShellRisk::High, "installs system software");
        }
        _ => return false,
    }
    true
}

fn install(ctx: &mut Ctx<'_>, risk: ShellRisk, reason: &str) {
    ctx.acc.imply(Action::PackageInstall);
    ctx.acc.network();
    ctx.acc.writes();
    ctx.raise(risk, reason);
}

fn first_operand(args: &[String]) -> Option<String> {
    args.iter()
        .find(|arg| !arg.starts_with('-'))
        .map(|arg| arg.to_ascii_lowercase())
}

fn node(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    let global =
        has_flag(args, &["-g", "--global"]) || (program == "yarn" && has_flag(args, &["global"]));
    match first_operand(args).as_deref() {
        None if program == "yarn" => install(ctx, ShellRisk::Medium, "installs project packages"),
        None => ctx.raise(ShellRisk::Low, "shows package-manager help"),
        Some(
            "install" | "i" | "add" | "ci" | "update" | "up" | "upgrade" | "in" | "isntall"
            | "rebuild" | "global",
        ) => {
            if global {
                install(ctx, ShellRisk::High, "installs packages globally");
            } else {
                install(ctx, ShellRisk::Medium, "installs project packages");
            }
        }
        Some("x" | "dlx" | "create" | "init" | "exec") => {
            install(ctx, ShellRisk::Medium, "downloads and runs a package");
        }
        Some("publish" | "unpublish" | "deprecate" | "dist-tag" | "owner") => {
            ctx.acc.imply(Action::NetworkRequest);
            ctx.acc.network();
            ctx.raise(
                ShellRisk::High,
                "publishes or changes a package in a registry",
            );
        }
        Some("login" | "adduser" | "token" | "logout") => {
            ctx.acc.credentials = true;
            ctx.acc.network();
            ctx.raise(ShellRisk::High, "manages package registry credentials");
        }
        Some("uninstall" | "remove" | "rm" | "un" | "r" | "unlink" | "prune") => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::Medium, "removes project packages");
        }
        Some(
            "view" | "info" | "ls" | "list" | "outdated" | "why" | "help" | "version" | "search"
            | "explain" | "pm",
        ) => {
            ctx.raise(ShellRisk::Low, "inspects packages");
        }
        Some("audit") => {
            ctx.acc.network();
            ctx.raise(ShellRisk::Low, "audits dependencies");
        }
        _ => ctx.raise(ShellRisk::Medium, "runs project scripts"),
    }
}

pub(super) fn pip_like(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    match first_operand(args).as_deref() {
        Some("install" | "add" | "sync" | "update" | "upgrade" | "inject" | "create") => {
            install(ctx, ShellRisk::Medium, "installs Python packages");
        }
        Some("download") => {
            ctx.acc.network();
            ctx.acc.writes();
            ctx.raise(ShellRisk::Medium, "downloads Python packages");
        }
        Some("uninstall" | "remove") => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::Medium, "removes Python packages");
        }
        Some("list" | "show" | "freeze" | "check" | "info" | "search" | "env") => {
            ctx.raise(ShellRisk::Low, "inspects Python packages");
        }
        _ if program == "poetry" || program == "pdm" => {
            ctx.raise(ShellRisk::Medium, "runs a project command");
        }
        _ => ctx.raise(ShellRisk::Medium, "runs a package-manager command"),
    }
}

fn cargo(args: &[String], ctx: &mut Ctx<'_>) {
    match first_operand(args).as_deref() {
        Some("install" | "add" | "update" | "fetch" | "vendor" | "generate-lockfile") => {
            install(ctx, ShellRisk::Medium, "installs or updates Rust crates");
        }
        Some("publish" | "yank" | "owner" | "login") => {
            ctx.acc.imply(Action::NetworkRequest);
            ctx.acc.network();
            ctx.raise(
                ShellRisk::High,
                "publishes or changes a crate in a registry",
            );
        }
        Some("uninstall" | "remove" | "rm") => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::Medium, "removes Rust crates");
        }
        Some(
            "version" | "tree" | "metadata" | "locate-project" | "pkgid" | "verify-project"
            | "help" | "search",
        )
        | None => ctx.raise(ShellRisk::Low, "inspects a Rust project"),
        Some(
            "build" | "b" | "check" | "c" | "test" | "t" | "clippy" | "fmt" | "doc" | "d" | "run"
            | "r" | "bench" | "clean" | "fix" | "new" | "init",
        ) => ctx.raise(ShellRisk::Medium, "runs a build, test, or developer tool"),
        Some(_) => ctx.raise(ShellRisk::Medium, "runs a cargo subcommand"),
    }
}

fn system(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    let user_level = matches!(program, "brew" | "scoop");
    let lower: Vec<String> = args.iter().map(|arg| arg.to_ascii_lowercase()).collect();
    let has = |names: &[&str]| lower.iter().any(|arg| names.contains(&arg.as_str()));
    if has(&[
        "install",
        "add",
        "-s",
        "-sy",
        "-syu",
        "-su",
        "upgrade",
        "update",
        "reinstall",
        "in",
        "i",
    ]) {
        let risk = if user_level {
            ShellRisk::Medium
        } else {
            ShellRisk::High
        };
        install(ctx, risk, "installs system packages");
    } else if has(&[
        "remove",
        "uninstall",
        "purge",
        "erase",
        "autoremove",
        "-r",
        "-rs",
        "-rns",
        "del",
    ]) {
        ctx.acc.writes();
        ctx.raise(ShellRisk::High, "removes system packages");
    } else if has(&[
        "list",
        "search",
        "info",
        "show",
        "query",
        "-q",
        "-ss",
        "-qi",
        "outdated",
        "doctor",
        "--version",
        "policy",
    ]) {
        ctx.raise(ShellRisk::Low, "inspects system packages");
    } else {
        ctx.raise(ShellRisk::High, "runs a system package manager");
    }
}
