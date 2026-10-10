//! Spawn `aidexd run --drive <root>` so it outlives its launcher.
//!
//! On Windows, `std::process::Command` always creates children with handle
//! inheritance enabled, so a daemon spawned that way would also inherit every
//! inheritable handle of the launcher, including a caller's stdout pipe
//! (`aidex daemon start --json | jq` would then never see EOF). Clearing the
//! inherit flag needs FFI, which this workspace forbids, so Windows launches go
//! through PowerShell `Start-Process` (ShellExecuteEx, no handle inheritance,
//! hidden window). Unix uses `Command` with a new process group: Unix file
//! descriptors are close-on-exec and stdio is replaced in the child.

use std::{io, path::Path, process::Child};

pub(super) enum Spawned {
    #[cfg_attr(windows, allow(dead_code))]
    Child(Child),
    #[cfg_attr(not(windows), allow(dead_code))]
    Pid(u32),
}

impl Spawned {
    /// `Some(status)` once the daemon is known to have exited.
    pub(super) fn exited(&mut self) -> Option<String> {
        match self {
            Self::Child(child) => child
                .try_wait()
                .ok()
                .flatten()
                .map(|status| status.to_string()),
            Self::Pid(_) => None,
        }
    }

    /// Best-effort termination of a daemon that never became ready.
    pub(super) fn kill(self) {
        match self {
            Self::Child(mut child) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            Self::Pid(pid) => kill_pid(pid),
        }
    }
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(not(windows))]
fn kill_pid(_pid: u32) {}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
pub(super) fn spawn(executable: &Path, root: &Path) -> io::Result<Spawned> {
    use std::os::windows::process::CommandExt;
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; $process = Start-Process -FilePath $env:AIDEX_LAUNCH_EXE -ArgumentList $env:AIDEX_LAUNCH_ARGS -WindowStyle Hidden -PassThru; [Console]::Out.Write($process.Id)";
    let root = root.to_string_lossy();
    let arguments = ["run", "--drive", root.as_ref()]
        .map(quote_windows_argument)
        .join(" ");
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ])
        .env("AIDEX_LAUNCH_EXE", executable)
        .env("AIDEX_LAUNCH_ARGS", arguments)
        .stdin(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.trim().parse::<u32>() {
        Ok(pid) if output.status.success() => Ok(Spawned::Pid(pid)),
        _ => Err(io::Error::other(format!(
            "Start-Process failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

#[cfg(not(windows))]
pub(super) fn spawn(executable: &Path, root: &Path) -> io::Result<Spawned> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(executable);
    command
        .arg("run")
        .arg("--drive")
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Keep terminal Ctrl-C and job-control signals away from the daemon.
        command.process_group(0);
    }
    command.spawn().map(Spawned::Child)
}

/// Quote one argument for the MSVC/`CommandLineToArgvW` command-line rules.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty() && !argument.contains([' ', '\t', '"']) {
        return argument.to_owned();
    }
    let mut quoted = String::from('"');
    let mut backslashes = 0;
    for character in argument.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            other => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(other);
                backslashes = 0;
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::quote_windows_argument;

    #[test]
    fn windows_arguments_survive_command_line_parsing_rules() {
        assert_eq!(quote_windows_argument("run"), "run");
        assert_eq!(quote_windows_argument(r"C:\plain\path"), r"C:\plain\path");
        assert_eq!(
            quote_windows_argument(r"C:\Users\A B\drive"),
            r#""C:\Users\A B\drive""#
        );
        // A trailing backslash must not escape the closing quote.
        assert_eq!(
            quote_windows_argument(r"H:\my drive\"),
            r#""H:\my drive\\""#
        );
        assert_eq!(quote_windows_argument(r#"a"b"#), r#""a\"b""#);
        assert_eq!(quote_windows_argument(""), r#""""#);
    }
}
