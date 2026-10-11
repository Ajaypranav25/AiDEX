//! Run a launched command with captured output and a deadline.

use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;

use crate::error::PackagerError;

/// One finished command.
#[derive(Debug, Serialize)]
pub struct Ran {
    pub args: Vec<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    #[serde(skip)]
    pub stdout: String,
    #[serde(skip)]
    pub stderr: String,
}

impl Ran {
    pub fn json(&self) -> Result<serde_json::Value, PackagerError> {
        serde_json::from_str(self.stdout.trim()).map_err(|error| {
            PackagerError::new(format!(
                "`{}` did not print JSON ({error}): {}{}",
                self.args.join(" "),
                self.stdout.trim(),
                self.stderr_suffix()
            ))
        })
    }

    pub fn stderr_suffix(&self) -> String {
        if self.stderr.trim().is_empty() {
            String::new()
        } else {
            format!("\nstderr: {}", self.stderr.trim())
        }
    }
}

/// Run `program args` in `cwd` with `env` added, killing it after `timeout`.
pub fn run(
    program: &Path,
    args: &[&str],
    cwd: &Path,
    env: &[(&str, &str)],
    timeout: Duration,
) -> Result<Ran, PackagerError> {
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            PackagerError::io(format_args!("cannot start `{}`", program.display()), error)
        })?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                return Err(PackagerError::io(
                    format_args!("cannot wait for `{}`", program.display()),
                    error,
                ));
            }
        }
    };
    let ran = Ran {
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        exit_code: status.and_then(|status| status.code()),
        duration_ms: started.elapsed().as_millis(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    };
    if status.is_none() {
        return Err(PackagerError::new(format!(
            "`{}` did not finish within {} s{}",
            ran.args.join(" "),
            timeout.as_secs(),
            ran.stderr_suffix()
        )));
    }
    Ok(ran)
}
