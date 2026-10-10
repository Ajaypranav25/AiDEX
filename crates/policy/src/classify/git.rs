//! Git subcommand classification.

use super::{
    net,
    programs::{Ctx, has_flag, operands, short_flags},
    shell::ShellRisk,
};
use crate::Action;

/// Global options that consume the following word.
const GLOBAL_WITH_VALUE: [&str; 6] = [
    "-C",
    "-c",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
];

pub(super) fn classify(program: &str, args: &[String], ctx: &mut Ctx<'_>) -> bool {
    if program != "git" {
        return false;
    }
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if !arg.starts_with('-') {
            break;
        }
        index += 1;
        if GLOBAL_WITH_VALUE.contains(&arg.as_str()) {
            index += 1;
        }
    }
    let Some(sub) = args.get(index) else {
        ctx.raise(ShellRisk::Low, "shows git help");
        return true;
    };
    subcommand(&sub.to_ascii_lowercase(), &args[index + 1..], ctx);
    true
}

fn subcommand(sub: &str, rest: &[String], ctx: &mut Ctx<'_>) {
    match sub {
        "status" | "diff" | "log" | "show" | "blame" | "annotate" | "shortlog" | "describe"
        | "rev-parse" | "rev-list" | "ls-files" | "ls-tree" | "grep" | "cat-file"
        | "whatchanged" | "help" | "version" | "--version" | "name-rev" | "for-each-ref"
        | "show-ref" | "count-objects" | "fsck" | "verify-commit" | "verify-tag"
        | "request-pull" | "range-diff" | "check-ignore" | "merge-base" => read(ctx),
        "push" => push(rest, ctx),
        "pull" | "fetch" | "clone" => {
            ctx.acc.imply(Action::GitPull);
            ctx.acc.network();
            ctx.acc.writes();
            net::remote_hosts(rest, ctx, 22);
            ctx.raise(ShellRisk::Medium, "downloads from a remote repository");
        }
        "ls-remote" => {
            ctx.acc.imply(Action::GitRead);
            ctx.acc.network();
            net::remote_hosts(rest, ctx, 22);
            ctx.raise(ShellRisk::Low, "lists references in a remote repository");
        }
        "submodule" => {
            if rest
                .first()
                .is_some_and(|op| matches!(op.as_str(), "update" | "add" | "sync" | "init"))
            {
                ctx.acc.imply(Action::GitPull);
                ctx.acc.network();
                ctx.acc.writes();
                ctx.raise(ShellRisk::Medium, "downloads submodules");
            } else {
                read(ctx);
            }
        }
        "remote" => match rest.first().map(String::as_str) {
            None | Some("-v" | "--verbose" | "show" | "get-url") => read(ctx),
            Some(_) => {
                ctx.acc.writes();
                ctx.raise(ShellRisk::Medium, "changes git remote configuration");
            }
        },
        "config" => config(rest, ctx),
        "commit" | "add" | "mv" | "rm" | "am" | "apply" | "notes" | "revert" | "cherry-pick"
        | "merge" | "rebase" | "commit-tree" | "mktag" | "update-index" | "init" => {
            ctx.acc.imply(Action::GitCommit);
            ctx.acc.writes();
            ctx.raise(ShellRisk::Medium, "records changes in git");
        }
        "checkout" | "switch" => {
            ctx.acc.imply(Action::GitBranch);
            ctx.acc.writes();
            let discards = has_flag(rest, &["-f", "--force", "--discard-changes", "--", "."]);
            if discards {
                ctx.raise(ShellRisk::High, "discards uncommitted changes");
            } else {
                ctx.raise(ShellRisk::Medium, "switches git branches");
            }
        }
        "restore" => {
            ctx.acc.writes();
            if has_flag(rest, &["--staged", "-S"]) && !has_flag(rest, &["--worktree", "-W"]) {
                ctx.raise(ShellRisk::Medium, "unstages changes");
            } else {
                ctx.raise(ShellRisk::High, "discards uncommitted changes");
            }
        }
        "reset" => {
            ctx.acc.imply(Action::GitCommit);
            ctx.acc.writes();
            if has_flag(rest, &["--hard"]) {
                ctx.raise(
                    ShellRisk::Destructive,
                    "discards uncommitted changes (git reset --hard)",
                );
            } else if has_flag(rest, &["--keep", "--merge"]) {
                ctx.raise(
                    ShellRisk::High,
                    "moves the branch and rewrites the working tree",
                );
            } else {
                ctx.raise(ShellRisk::Medium, "moves the branch or unstages changes");
            }
        }
        "clean" => clean(rest, ctx),
        "branch" => branch(rest, ctx),
        "tag" => {
            if operands(rest).next().is_none() || has_flag(rest, &["-l", "--list"]) {
                read(ctx);
            } else {
                ctx.acc.imply(Action::GitBranch);
                ctx.acc.writes();
                ctx.raise(ShellRisk::Medium, "creates or deletes git tags");
            }
        }
        "stash" => match rest.first().map(String::as_str) {
            Some("list" | "show") => read(ctx),
            Some("drop" | "clear") => {
                ctx.acc.writes();
                ctx.raise(ShellRisk::High, "discards stashed changes");
            }
            _ => {
                ctx.acc.imply(Action::GitCommit);
                ctx.acc.writes();
                ctx.raise(ShellRisk::Medium, "stashes or restores changes");
            }
        },
        "worktree" => {
            if rest.first().is_some_and(|op| op == "list") {
                read(ctx);
            } else {
                ctx.acc.imply(Action::GitBranch);
                ctx.acc.writes();
                ctx.raise(ShellRisk::Medium, "manages git worktrees");
            }
        }
        "reflog" => {
            if rest
                .first()
                .is_some_and(|op| op == "expire" || op == "delete")
            {
                ctx.acc.writes();
                ctx.raise(ShellRisk::High, "rewrites or prunes repository history");
            } else {
                read(ctx);
            }
        }
        "gc" | "prune" | "filter-branch" | "filter-repo" | "replace" | "update-ref"
        | "symbolic-ref" => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::High, "rewrites or prunes repository history");
        }
        "send-email" => {
            ctx.acc.imply(Action::NetworkRequest);
            ctx.acc.network();
            ctx.raise(ShellRisk::High, "sends patches by email");
        }
        "lfs" => {
            ctx.acc.network();
            ctx.acc.writes();
            ctx.raise(ShellRisk::Medium, "transfers large files");
        }
        _ => ctx.raise(ShellRisk::Medium, "unrecognized git subcommand"),
    }
}

fn read(ctx: &mut Ctx<'_>) {
    ctx.acc.imply(Action::GitRead);
    ctx.raise(ShellRisk::Low, "reads git history or status");
}

fn push(rest: &[String], ctx: &mut Ctx<'_>) {
    ctx.acc.imply(Action::GitPush);
    ctx.acc.network();
    net::remote_hosts(rest, ctx, 22);
    let rewrites = short_flags(rest).any(|flag| flag == 'f' || flag == 'd')
        || rest.iter().any(|arg| {
            arg.starts_with("--force")
                || matches!(arg.as_str(), "--mirror" | "--delete" | "--prune")
                || (arg.starts_with('+') && arg.len() > 1)
                || (arg.starts_with(':') && arg.len() > 1)
        });
    if rewrites {
        ctx.raise(
            ShellRisk::Destructive,
            "force-pushes or deletes remote history",
        );
    } else {
        ctx.raise(ShellRisk::High, "uploads commits to a remote repository");
    }
}

fn config(rest: &[String], ctx: &mut Ctx<'_>) {
    let reads = has_flag(
        rest,
        &[
            "--get",
            "--get-all",
            "--get-regexp",
            "--list",
            "-l",
            "--show-origin",
            "--get-urlmatch",
        ],
    ) || operands(rest).count() == 1;
    if reads {
        read(ctx);
        return;
    }
    ctx.acc.writes();
    let lower: Vec<String> = rest.iter().map(|arg| arg.to_ascii_lowercase()).collect();
    let executes = lower.iter().any(|key| {
        key.starts_with("alias.")
            || key.starts_with("core.hookspath")
            || key.starts_with("core.sshcommand")
            || key.starts_with("core.fsmonitor")
            || key.starts_with("core.pager")
            || key.starts_with("core.editor")
            || key.ends_with(".helper")
    });
    if executes {
        ctx.raise(ShellRisk::High, "configures a command git will execute");
    } else {
        ctx.raise(ShellRisk::Medium, "changes git configuration");
    }
}

fn clean(rest: &[String], ctx: &mut Ctx<'_>) {
    let force = short_flags(rest).any(|flag| flag == 'f') || has_flag(rest, &["--force"]);
    let dry_run = short_flags(rest).any(|flag| flag == 'n') || has_flag(rest, &["--dry-run"]);
    if dry_run {
        ctx.raise(ShellRisk::Low, "lists untracked files (dry run)");
    } else if force {
        ctx.acc.writes();
        ctx.raise(
            ShellRisk::Destructive,
            "permanently deletes untracked files (git clean -f)",
        );
    } else {
        ctx.raise(ShellRisk::Medium, "runs git clean without --force");
    }
}

fn branch(rest: &[String], ctx: &mut Ctx<'_>) {
    let force_delete = rest.iter().any(|arg| arg == "-D")
        || (has_flag(rest, &["-d", "--delete"]) && has_flag(rest, &["-f", "--force"]));
    let modifies = has_flag(
        rest,
        &[
            "-d",
            "--delete",
            "-m",
            "-M",
            "-c",
            "-C",
            "--move",
            "--copy",
            "-u",
            "--set-upstream-to",
        ],
    );
    if force_delete {
        ctx.acc.imply(Action::GitBranch);
        ctx.acc.writes();
        ctx.raise(ShellRisk::High, "force-deletes a git branch");
    } else if modifies || operands(rest).next().is_some() {
        ctx.acc.imply(Action::GitBranch);
        ctx.acc.writes();
        ctx.raise(
            ShellRisk::Medium,
            "creates, renames, or deletes a git branch",
        );
    } else {
        read(ctx);
    }
}
