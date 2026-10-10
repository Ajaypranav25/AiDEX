//! Network tools, remote shells, destination extraction, and credential references.

use super::{
    programs::{Ctx, SegmentFacts, has_flag, normalize_program},
    shell::{Accumulator, ShellRisk},
};
use crate::Action;

const FETCHERS: [&str; 18] = [
    "curl",
    "wget",
    "wget2",
    "iwr",
    "invoke-webrequest",
    "irm",
    "invoke-restmethod",
    "aria2c",
    "http",
    "https",
    "xh",
    "httpie",
    "start-bitstransfer",
    "bitsadmin",
    "lwp-download",
    "ftp",
    "tftp",
    "fetch",
];

const REMOTE_SHELLS: [&str; 14] = [
    "ssh",
    "scp",
    "sftp",
    "mosh",
    "plink",
    "pscp",
    "telnet",
    "nc",
    "ncat",
    "netcat",
    "socat",
    "enter-pssession",
    "new-pssession",
    "ssh-copy-id",
];

/// Options (curl/wget, case-sensitive) whose next word is a value, not a destination.
const FETCH_VALUE_OPTIONS: [&str; 42] = [
    "-o",
    "-H",
    "-d",
    "-X",
    "-u",
    "-A",
    "-e",
    "-T",
    "-F",
    "-b",
    "-c",
    "-K",
    "-w",
    "-m",
    "-E",
    "-r",
    "-P",
    "-U",
    "-t",
    "-i",
    "--output",
    "--header",
    "--data",
    "--data-raw",
    "--data-binary",
    "--data-urlencode",
    "--request",
    "--user",
    "--user-agent",
    "--referer",
    "--upload-file",
    "--form",
    "--cookie",
    "--cookie-jar",
    "--config",
    "--write-out",
    "--max-time",
    "--cert",
    "--key",
    "--cacert",
    "--post-data",
    "--post-file",
];

/// PowerShell web-cmdlet parameters (case-insensitive) whose next word is a value.
const POWERSHELL_VALUE_OPTIONS: [&str; 10] = [
    "-outfile",
    "-method",
    "-headers",
    "-body",
    "-infile",
    "-useragent",
    "-credential",
    "-contenttype",
    "-timeoutsec",
    "-destination",
];

pub(super) fn classify(program: &str, args: &[String], ctx: &mut Ctx<'_>) -> bool {
    if FETCHERS.contains(&program)
        || (program == "certutil"
            && (has_flag(
                args,
                &["-urlcache", "/urlcache", "-verifyctl", "/verifyctl"],
            ) || args.iter().any(|arg| url_target(arg).is_some())))
    {
        fetch(program, args, ctx);
        return true;
    }
    if REMOTE_SHELLS.contains(&program) {
        remote(program, args, ctx);
        return true;
    }
    false
}

fn fetch(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    ctx.acc.imply(Action::NetworkRequest);
    ctx.acc.network();
    ctx.acc.fetcher = true;
    ctx.facts.fetcher = true;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        index += 1;
        let lower = arg.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "-x" | "--proxy" | "-uri" | "-proxy" | "-source"
        ) {
            if let Some(value) = args.get(index) {
                destination(value, ctx);
                index += 1;
            }
            continue;
        }
        if FETCH_VALUE_OPTIONS.contains(&arg.as_str())
            || POWERSHELL_VALUE_OPTIONS.contains(&lower.as_str())
        {
            index += 1;
            continue;
        }
        if !arg.starts_with('-') && !arg.starts_with('/') {
            destination(arg, ctx);
        }
    }
    let lower: Vec<String> = args.iter().map(|arg| arg.to_ascii_lowercase()).collect();
    let has = |names: &[&str]| lower.iter().any(|arg| names.contains(&arg.as_str()));
    let exact = |names: &[&str]| args.iter().any(|arg| names.contains(&arg.as_str()));
    let writes = match program {
        "curl" => exact(&[
            "-o",
            "-O",
            "--output",
            "--remote-name",
            "--remote-name-all",
            "-J",
        ]),
        "wget" | "wget2" => !has(&["--spider"]),
        "iwr" | "invoke-webrequest" | "irm" | "invoke-restmethod" => has(&["-outfile"]),
        "aria2c" | "start-bitstransfer" | "bitsadmin" | "lwp-download" | "certutil" | "ftp"
        | "tftp" | "fetch" => true,
        _ => false,
    };
    if writes {
        ctx.acc.writes();
    }
    let uploads = exact(&["-d", "-F", "-T", "--upload-file", "--json"])
        || args.iter().any(|arg| {
            ["--data", "--form", "--post-", "--body-"]
                .iter()
                .any(|prefix| arg.starts_with(prefix))
        })
        || has(&["-body", "-infile"])
        || args.windows(2).any(|pair| {
            (pair[0] == "-X"
                || matches!(
                    pair[0].to_ascii_lowercase().as_str(),
                    "--request" | "-method" | "--method"
                ))
                && matches!(
                    pair[1].to_ascii_lowercase().as_str(),
                    "post" | "put" | "patch" | "delete"
                )
        });
    if uploads {
        ctx.raise(ShellRisk::High, "sends data to a remote server");
    } else {
        ctx.raise(ShellRisk::Medium, "downloads or requests remote content");
    }
}

fn destination(word: &str, ctx: &mut Ctx<'_>) {
    if let Some((host, port)) = url_target(word) {
        ctx.acc.target(&host, port);
    } else if let Some((host, port)) = bare_host(word) {
        ctx.acc.target(&host, port.unwrap_or(443));
    }
}

fn remote(program: &str, args: &[String], ctx: &mut Ctx<'_>) {
    ctx.acc.imply(Action::NetworkRequest);
    ctx.acc.network();
    ctx.raise(
        ShellRisk::High,
        "opens a remote shell or transfers files to another host",
    );
    const WITH_VALUE: [&str; 24] = [
        "-p", "-P", "-i", "-l", "-o", "-F", "-J", "-L", "-R", "-D", "-b", "-c", "-e", "-m", "-O",
        "-Q", "-S", "-W", "-w", "-E", "-B", "-I", "-s", "-x",
    ];
    let mut port = None;
    let mut operands = Vec::new();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        index += 1;
        if arg.eq_ignore_ascii_case("-computername") || arg.eq_ignore_ascii_case("-cn") {
            if let Some(value) = args.get(index) {
                ctx.acc.target(value, 5985);
                index += 1;
            }
        } else if WITH_VALUE.contains(&arg.as_str()) {
            if matches!(arg.as_str(), "-p" | "-P") {
                port = args.get(index).and_then(|value| value.parse::<u16>().ok());
            }
            index += 1;
        } else if !arg.starts_with('-') {
            operands.push(arg.as_str());
        }
    }
    let default_port = match program {
        "telnet" => 23,
        "nc" | "ncat" | "netcat" | "socat" => 0,
        _ => 22,
    };
    for (position, operand) in operands.iter().enumerate() {
        if let Some((host, url_port)) = url_target(operand) {
            ctx.acc.target(&host, url_port);
        } else if let Some(host) = scp_host(operand) {
            ctx.acc.target(&host, port.unwrap_or(default_port));
        } else if let Some(address) = socat_address(operand) {
            ctx.acc.target(&address.0, address.1);
        } else if position == 0 && !matches!(program, "scp" | "pscp") {
            let host = operand.rsplit_once('@').map_or(*operand, |(_, host)| host);
            let netcat_port = operands.get(1).and_then(|value| value.parse::<u16>().ok());
            ctx.acc
                .target(host, port.or(netcat_port).unwrap_or(default_port));
        }
    }
}

/// Record destinations for `git`/`rsync` style remote arguments.
pub(super) fn remote_hosts(args: &[String], ctx: &mut Ctx<'_>, default_port: u16) {
    for arg in args {
        if let Some((host, port)) = url_target(arg) {
            ctx.acc.target(&host, port);
        } else if let Some(host) = scp_host(arg) {
            ctx.acc.target(&host, default_port);
        }
    }
}

/// Signals carried by individual words regardless of program: credential paths,
/// in-line .NET downloads, and base64 decoding.
pub(super) fn scan_words(words: &[String], acc: &mut Accumulator, facts: &mut SegmentFacts) {
    let ssh_like = words.iter().any(|word| {
        matches!(
            normalize_program(word).as_str(),
            "ssh" | "scp" | "sftp" | "plink" | "pscp" | "mosh" | "rsync" | "ssh-add" | "ssh-keygen"
        )
    });
    let mut previous: Option<&str> = None;
    for word in words {
        let identity_option = ssh_like && previous.is_some_and(|prev| prev == "-i" || prev == "-f");
        previous = Some(word);
        let lower = word.to_ascii_lowercase();
        if !identity_option && references_credentials(&lower) {
            acc.credentials = true;
            acc.raise(ShellRisk::High, "references credential material");
        }
        if lower.contains("frombase64string") {
            acc.decoder = true;
            facts.decoder = true;
        }
        if [
            "downloadstring",
            "downloadfile",
            "downloaddata",
            "net.webclient",
            "net.http.httpclient",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
        {
            acc.fetcher = true;
            facts.fetcher = true;
            acc.network();
            acc.imply(Action::NetworkRequest);
            if let Some((host, port)) = url_target(word) {
                acc.target(&host, port);
            }
        }
    }
}

fn references_credentials(lower: &str) -> bool {
    let path = lower.replace('\\', "/");
    let path = path.trim_matches(['"', '\'', '(', ')', ',', ';']);
    if path.ends_with(".pub") {
        return false;
    }
    const NEEDLES: [&str; 20] = [
        ".ssh/id_",
        ".aws/credentials",
        ".aws/config",
        ".git-credentials",
        ".netrc",
        "_netrc",
        ".npmrc",
        ".pypirc",
        ".docker/config.json",
        ".kube/config",
        "/etc/shadow",
        ".gnupg/",
        "credentials.json",
        "login.keychain",
        "/library/keychains",
        "microsoft/credentials",
        "microsoft/protect",
        ".vault-token",
        ".config/gh/hosts.yml",
        "application_default_credentials",
    ];
    if NEEDLES.iter().any(|needle| path.contains(needle)) {
        return true;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if matches!(name, "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa") {
        return true;
    }
    let env_file = name == ".env"
        || (name.starts_with(".env.")
            && !matches!(
                name,
                ".env.example" | ".env.sample" | ".env.template" | ".env.dist"
            ));
    const SECRET_EXTENSIONS: [&str; 7] =
        [".pem", ".key", ".p12", ".pfx", ".kdbx", ".keystore", ".jks"];
    env_file
        || SECRET_EXTENSIONS
            .iter()
            .any(|extension| name.ends_with(extension))
}

/// `scheme://[user@]host[:port]/...` anywhere in a word. `file://` names no host.
pub(super) fn url_target(word: &str) -> Option<(String, u16)> {
    let lower = word.to_ascii_lowercase();
    let index = lower.find("://")?;
    let scheme_start = lower[..index]
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')))
        .map_or(0, |position| position + 1);
    let scheme = &lower[scheme_start..index];
    let rest = &lower[index + 3..];
    let end = rest
        .find(['/', '?', '#', '\'', '"', ')', ' ', '\\', ';'])
        .unwrap_or(rest.len());
    let authority = &rest[..end];
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port) = split_host_port(host_port)?;
    let default = match scheme {
        "file" | "" => return None,
        "http" | "ws" => 80,
        "https" | "wss" => 443,
        "ftp" => 21,
        "ftps" => 990,
        "ssh" | "sftp" | "git+ssh" | "ssh+git" | "scp" => 22,
        "git" => 9418,
        "rsync" => 873,
        "smb" => 445,
        _ => 0,
    };
    Some((host, port.unwrap_or(default)))
}

/// `host`, `host/path`, or `host:port/path` without a scheme (curl's default).
fn bare_host(word: &str) -> Option<(String, Option<u16>)> {
    if word.contains("://") || word.starts_with(['-', '@', '.', '/', '\\', '~', '$']) {
        return None;
    }
    let authority = word.split('/').next().unwrap_or(word);
    if !authority.contains('.') && authority != "localhost" {
        return None;
    }
    let (host, port) = split_host_port(authority)?;
    let has_letters_or_ip =
        host.chars().any(|ch| ch.is_ascii_alphabetic()) || host.parse::<std::net::IpAddr>().is_ok();
    has_letters_or_ip.then_some((host, port))
}

/// `[user@]host:path` (scp/rsync/git syntax), excluding Windows drive paths.
fn scp_host(word: &str) -> Option<String> {
    if word.contains("://") || word.starts_with('-') {
        return None;
    }
    let (left, _) = word.split_once(':')?;
    let host = left.rsplit_once('@').map_or(left, |(_, host)| host);
    (host.len() > 1 && !host.contains(['/', '\\', '[', '$', '%'])).then(|| host.to_owned())
}

/// `TCP:host:port` style socat addresses.
fn socat_address(word: &str) -> Option<(String, u16)> {
    let lower = word.to_ascii_lowercase();
    let rest = ["tcp:", "tcp4:", "tcp6:", "udp:", "openssl:", "tcp-connect:"]
        .iter()
        .find_map(|prefix| lower.strip_prefix(prefix))?;
    let (host, port) = rest.split(',').next()?.rsplit_once(':')?;
    Some((host.to_owned(), port.parse().ok()?))
}

fn split_host_port(value: &str) -> Option<(String, Option<u16>)> {
    if value.is_empty() {
        return None;
    }
    if let Some(inner) = value.strip_prefix('[') {
        let (host, rest) = inner.split_once(']')?;
        let port = rest.strip_prefix(':').and_then(|port| port.parse().ok());
        return Some((host.to_owned(), port));
    }
    match value.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => Some((host.to_owned(), port.parse().ok())),
        Some(_) => Some((value.to_owned(), None)),
        None => Some((value.to_owned(), None)),
    }
}
