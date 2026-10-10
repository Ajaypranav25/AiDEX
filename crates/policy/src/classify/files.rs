//! Deletion, disk, raw-device, permission, and redirection classification.

use super::{
    net,
    programs::{Ctx, classify_words, has_flag, operands, short_flags},
    shell::{Accumulator, ShellRisk},
};
use crate::Action;

pub(super) fn classify(program: &str, args: &[String], ctx: &mut Ctx<'_>) -> bool {
    match program {
        "rm" | "remove-item" | "ri" | "del" | "erase" | "rd" | "rmdir" => {
            remove(program, args, ctx);
        }
        "shred" | "srm" | "wipe" => {
            ctx.acc.writes();
            if operands(args).any(|target| is_block_device(target)) {
                raw_write(ctx);
            }
            ctx.raise(ShellRisk::Destructive, "irreversibly overwrites files");
        }
        "unlink" | "truncate" => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::High, "deletes or truncates files");
        }
        "find" => find(args, ctx),
        "rsync" => {
            ctx.acc.writes();
            net::remote_hosts(args, ctx, 22);
            if has_flag(
                args,
                &["--delete", "--delete-before", "--delete-after", "--del"],
            ) {
                ctx.raise(ShellRisk::High, "deletes files missing from the source");
            } else {
                ctx.raise(ShellRisk::Medium, "synchronizes files");
            }
        }
        "robocopy" => {
            ctx.acc.writes();
            if has_flag(args, &["/mir", "/purge", "-mir", "-purge"]) {
                ctx.raise(
                    ShellRisk::Destructive,
                    "mirrors directories, deleting files missing from the source",
                );
            } else {
                ctx.raise(ShellRisk::Medium, "copies files");
            }
        }
        "dd" => dd(args, ctx),
        "blkdiscard" => {
            raw_write(ctx);
            ctx.raise(ShellRisk::Destructive, "discards every block on a device");
        }
        "format" | "format-volume" | "clear-disk" | "initialize-disk" | "remove-partition"
        | "new-partition" | "diskpart" | "mke2fs" | "mkswap" | "wipefs" | "cfdisk" | "newfs"
        | "mkdosfs" | "mkntfs" | "mformat" => disk_format(ctx),
        mkfs if mkfs.starts_with("mkfs") => disk_format(ctx),
        "fdisk" | "sfdisk" | "gdisk" | "sgdisk" | "parted" => {
            let listing = has_flag(args, &["-l", "--list", "print", "-p"])
                && !has_flag(
                    args,
                    &["mklabel", "mkpart", "rm", "resizepart", "-z", "--zap-all"],
                );
            if listing {
                ctx.raise(ShellRisk::High, "inspects disk partition tables");
            } else {
                disk_format(ctx);
            }
        }
        "diskutil" => diskutil(args, ctx),
        "vssadmin" | "wbadmin" => {
            if has_flag(args, &["delete"]) {
                ctx.acc.writes();
                ctx.raise(ShellRisk::Destructive, "deletes backups or shadow copies");
            } else {
                ctx.raise(ShellRisk::High, "manages system backups");
            }
        }
        "cipher" if has_flag(args, &["/w"]) => {
            ctx.acc.writes();
            ctx.raise(ShellRisk::High, "wipes free disk space");
        }
        "chmod" | "chown" | "chgrp" | "icacls" | "cacls" | "takeown" | "set-acl" => {
            permissions(program, args, ctx);
        }
        _ => return false,
    }
    true
}

/// Classify an output redirection target.
pub(super) fn redirect(target: &str, acc: &mut Accumulator) {
    let lower = target.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "/dev/null" | "nul" | "nul:" | "$null" | "/dev/stdout" | "/dev/stderr"
    ) {
        return;
    }
    acc.writes();
    if is_block_device(target) {
        acc.imply(Action::DriveRawWrite);
        acc.raise(ShellRisk::Destructive, "writes directly to a block device");
    } else {
        acc.raise(ShellRisk::Medium, "redirects output to a file");
    }
}

fn remove(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    let lower: Vec<String> = args.iter().map(|arg| arg.to_ascii_lowercase()).collect();
    let windows_switch = matches!(program, "del" | "erase" | "rd" | "rmdir");
    let recursive = short_flags(args).any(|flag| flag == 'r' || flag == 'R')
        || lower
            .iter()
            .any(|arg| arg == "--recursive" || arg.starts_with("-rec") || arg == "/s");
    let targets: Vec<&String> = args
        .iter()
        .filter(|arg| {
            let switch = windows_switch && arg.starts_with('/') && arg.len() <= 3;
            !(arg.starts_with('-') || switch)
        })
        .collect();
    let wildcard = targets.iter().any(|target| target.contains('*'));
    let arbitrary = has_flag(args, &["--no-preserve-root"])
        || ((recursive || wildcard) && targets.iter().any(|target| is_root_like(target)));
    ctx.acc.writes();
    if arbitrary {
        ctx.acc.imply(Action::DirectoryDeleteArbitrary);
        ctx.raise(
            ShellRisk::Destructive,
            "deletes a root, home, or system directory",
        );
    }
    if recursive {
        ctx.raise(ShellRisk::Destructive, "recursively deletes files");
    } else if matches!(program, "rd" | "rmdir") {
        ctx.raise(ShellRisk::Medium, "removes empty directories");
    } else {
        ctx.raise(ShellRisk::High, "deletes files");
    }
    if targets
        .iter()
        .any(|target| target.starts_with('$') || target.contains('%'))
    {
        ctx.raise(ShellRisk::High, "deletion target is computed at runtime");
    }
}

fn find(args: &[String], ctx: &mut Ctx<'_>) {
    if has_flag(args, &["-delete"]) {
        ctx.acc.writes();
        if args
            .iter()
            .take_while(|arg| !arg.starts_with('-'))
            .any(|root| is_root_like(root))
        {
            ctx.acc.imply(Action::DirectoryDeleteArbitrary);
            ctx.raise(
                ShellRisk::Destructive,
                "deletes a root, home, or system directory",
            );
        }
        ctx.raise(ShellRisk::High, "deletes matched files");
    }
    let exec = args
        .iter()
        .position(|arg| matches!(arg.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir"));
    if let Some(start) = exec {
        let command = &args[start + 1..];
        let end = command
            .iter()
            .position(|arg| arg == ";" || arg == "+")
            .unwrap_or(command.len());
        let command: Vec<String> = command[..end]
            .iter()
            .filter(|arg| arg.as_str() != "{}")
            .cloned()
            .collect();
        classify_words(&command, ctx);
    } else if !has_flag(args, &["-delete"]) {
        ctx.raise(ShellRisk::Low, "searches for files");
    }
}

fn dd(args: &[String], ctx: &mut Ctx<'_>) {
    let mut output = false;
    for arg in args {
        if let Some(target) = arg.strip_prefix("of=") {
            output = true;
            ctx.acc.writes();
            if is_block_device(target) {
                raw_write(ctx);
                ctx.raise(ShellRisk::Destructive, "writes raw data to a block device");
            } else {
                ctx.raise(ShellRisk::Medium, "writes a file with dd");
            }
        } else if let Some(source) = arg.strip_prefix("if=") {
            if is_block_device(source) && !source.contains("zero") && !source.contains("random") {
                ctx.raise(ShellRisk::High, "reads a raw block device");
            }
        }
    }
    if !output {
        ctx.raise(ShellRisk::Medium, "copies data with dd");
    }
}

fn diskutil(args: &[String], ctx: &mut Ctx<'_>) {
    const FORMATTING: [&str; 10] = [
        "erasedisk",
        "erasevolume",
        "partitiondisk",
        "reformat",
        "zerodisk",
        "randomdisk",
        "secureerase",
        "splitpartition",
        "mergepartitions",
        "deletecontainer",
    ];
    let lower: Vec<String> = args.iter().map(|arg| arg.to_ascii_lowercase()).collect();
    if lower
        .iter()
        .any(|arg| FORMATTING.contains(&arg.as_str()) || arg.starts_with("delete"))
    {
        disk_format(ctx);
    } else if lower
        .first()
        .is_some_and(|sub| matches!(sub.as_str(), "list" | "info" | "activity"))
    {
        ctx.raise(ShellRisk::Low, "inspects disks");
    } else {
        ctx.acc.writes();
        ctx.raise(ShellRisk::High, "changes disk or volume state");
    }
}

fn permissions(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    let lower: Vec<String> = args.iter().map(|arg| arg.to_ascii_lowercase()).collect();
    match program {
        "chmod" => {
            ctx.acc.writes();
            let recursive =
                short_flags(args).any(|flag| flag == 'R') || has_flag(args, &["--recursive"]);
            const WORLD: [&str; 9] = [
                "777", "0777", "666", "0666", "a+rwx", "a+w", "o+w", "ugo+rwx", "o+rwx",
            ];
            let world = lower.iter().any(|arg| WORLD.contains(&arg.as_str()));
            let sensitive = recursive && operands(args).any(|target| is_root_like(target));
            if (recursive && world) || sensitive {
                ctx.raise(
                    ShellRisk::High,
                    "makes files world-writable or changes permissions of a system directory",
                );
            } else {
                ctx.raise(ShellRisk::Medium, "changes file permissions");
            }
        }
        "chown" | "chgrp" => {
            ctx.acc.writes();
            if short_flags(args).any(|flag| flag == 'R') || has_flag(args, &["--recursive"]) {
                ctx.raise(ShellRisk::High, "changes file ownership recursively");
            } else {
                ctx.raise(ShellRisk::Medium, "changes file ownership");
            }
        }
        "icacls" | "cacls" => {
            let modifies = lower.iter().any(|arg| {
                [
                    "/grant",
                    "/deny",
                    "/remove",
                    "/setowner",
                    "/reset",
                    "/inheritance",
                    "/e",
                    "/g",
                ]
                .iter()
                .any(|switch| arg.starts_with(switch))
            });
            if !modifies {
                ctx.raise(ShellRisk::Low, "reads access control lists");
                return;
            }
            ctx.acc.writes();
            let broad = lower.iter().any(|arg| {
                arg.contains("everyone") || arg.contains("*s-1-1-0") || arg.contains("users:")
            });
            if broad && has_flag(args, &["/t"]) {
                ctx.raise(ShellRisk::High, "grants broad access recursively");
            } else {
                ctx.raise(ShellRisk::Medium, "changes access control lists");
            }
        }
        _ => {
            ctx.acc.writes();
            ctx.raise(
                ShellRisk::High,
                "takes ownership or replaces access control lists",
            );
        }
    }
}

fn disk_format(ctx: &mut Ctx<'_>) {
    ctx.acc.imply(Action::DiskFormat);
    ctx.acc.writes();
    ctx.raise(ShellRisk::Destructive, "formats or repartitions a disk");
}

fn raw_write(ctx: &mut Ctx<'_>) {
    ctx.acc.imply(Action::DriveRawWrite);
    ctx.acc.writes();
}

fn normalize(path: &str) -> String {
    path.trim_matches(['"', '\''])
        .replace('\\', "/")
        .to_ascii_lowercase()
}

/// Block devices and raw volume paths (`/dev/sda`, `\\.\PhysicalDrive0`).
pub(super) fn is_block_device(path: &str) -> bool {
    let path = normalize(path);
    if let Some(device) = path.strip_prefix("/dev/") {
        const HARMLESS: [&str; 9] = [
            "null", "zero", "random", "urandom", "tty", "stdout", "stderr", "stdin", "full",
        ];
        return !(HARMLESS.contains(&device)
            || device.starts_with("fd/")
            || device.starts_with("pts/")
            || device.starts_with("tty"));
    }
    path.starts_with("//./")
        || path.starts_with("//?/volume{")
        || path.starts_with("//?/globalroot")
}

/// Filesystem roots, drive roots, system directories, home directories and their
/// immediate children, and parent-only paths (`../..`).
pub(super) fn is_root_like(path: &str) -> bool {
    let normalized = normalize(path);
    let absolute = normalized.starts_with('/');
    let mut stripped = normalized.as_str();
    loop {
        let next = stripped.trim_end_matches(['*', '/']).trim_end_matches("/.");
        if next == stripped {
            break;
        }
        stripped = next;
    }
    if stripped.is_empty() {
        return absolute;
    }
    if is_drive(stripped) {
        return true;
    }
    if stripped.split('/').all(|part| part == "..") {
        return true;
    }
    const HOME: [&str; 10] = [
        "~",
        "$home",
        "${home}",
        "%userprofile%",
        "$env:userprofile",
        "%homepath%",
        "$env:homepath",
        "%homedrive%%homepath%",
        "$env:home",
        "%home%",
    ];
    const SYSTEM: [&str; 34] = [
        "/home",
        "/users",
        "/root",
        "/etc",
        "/usr",
        "/var",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/boot",
        "/opt",
        "/system",
        "/library",
        "/applications",
        "/dev",
        "/proc",
        "/sys",
        "/mnt",
        "/media",
        "/volumes",
        "/private",
        "/srv",
        "%systemroot%",
        "%windir%",
        "$env:systemroot",
        "$env:windir",
        "%programfiles%",
        "$env:programfiles",
        "%appdata%",
        "$env:appdata",
        "%localappdata%",
        "$env:localappdata",
        "%programdata%",
    ];
    if HOME.contains(&stripped) || SYSTEM.contains(&stripped) {
        return true;
    }
    if stripped.starts_with('~') && !stripped.contains('/') {
        return true;
    }
    let parts: Vec<&str> = stripped.split('/').collect();
    if let [drive, top] = parts.as_slice() {
        if is_drive(drive)
            && matches!(
                *top,
                "windows" | "users" | "program files" | "program files (x86)" | "programdata"
            )
        {
            return true;
        }
    }
    if let Some((first, rest)) = parts.split_first() {
        // Immediate children of a home directory (`~/Documents`).
        if (HOME.contains(first)) && rest.len() == 1 {
            return true;
        }
    }
    match parts.as_slice() {
        ["", "home" | "users", _] | ["", "home" | "users", _, _] => true,
        [drive, "users", _] | [drive, "users", _, _] => is_drive(drive),
        _ => false,
    }
}

fn is_drive(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
