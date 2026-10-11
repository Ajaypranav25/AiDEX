//! MCP over a sandboxed process's stdin/stdout: newline-delimited JSON-RPC,
//! at most 1 MiB per frame. A reader thread drains the process continuously so a
//! chatty server never fills its pipes: stderr goes to the log buffer, server
//! notifications and requests are handled in place, and responses are handed to
//! the waiting caller.

use std::{
    sync::{
        Arc,
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use aidex_core::CancelToken;
use aidex_sandbox::ManagedProcess;
use serde_json::Value;

use super::{
    Channel, cancelled_error,
    shared::{Routed, Shared, id_matches, route},
};
use crate::{LogStream, McpError, McpErrorKind, redact::Scrubber, redact::bounded_redacted};

/// Largest single protocol frame.
pub const MAX_FRAME: usize = 1024 * 1024;
const POLL: Duration = Duration::from_millis(10);
/// How long a server may take to exit after its stdin closes.
const GRACEFUL_EXIT: Duration = Duration::from_secs(2);

type Delivery = Result<(Value, Result<Value, McpError>), McpError>;

pub(crate) struct StdioChannel {
    process: Arc<ManagedProcess>,
    shared: Arc<Shared>,
    responses: Receiver<Delivery>,
    reader: Option<JoinHandle<()>>,
}

impl StdioChannel {
    pub fn start(
        process: ManagedProcess,
        shared: Arc<Shared>,
        scrubber: Scrubber,
    ) -> Result<Self, McpError> {
        let process = Arc::new(process);
        let (sender, responses) = channel();
        let reader = {
            let reading = Arc::clone(&process);
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("aidex-mcp-stdio".into())
                .spawn(move || read_loop(&reading, &shared, &scrubber, &sender))
                .map_err(|_| {
                    let _ = process.kill_tree();
                    McpError::new(
                        McpErrorKind::Unavailable,
                        "cannot start the MCP reader thread",
                    )
                })?
        };
        Ok(Self {
            process,
            shared,
            responses,
            reader: Some(reader),
        })
    }

    pub fn process(&self) -> Arc<ManagedProcess> {
        Arc::clone(&self.process)
    }

    fn disconnected(&self) -> McpError {
        self.shared.ended().unwrap_or_else(|| {
            McpError::new(McpErrorKind::Disconnected, "the MCP server is not running")
        })
    }
}

impl Channel for StdioChannel {
    fn send(&mut self, message: &Value, _cancel: &CancelToken) -> Result<(), McpError> {
        if !self.shared.is_alive() {
            return Err(self.disconnected());
        }
        let mut line = serde_json::to_vec(message)
            .map_err(|_| McpError::protocol("cannot encode the MCP message"))?;
        if line.len() > MAX_FRAME {
            return Err(McpError::new(
                McpErrorKind::OutputLimit,
                "the MCP request exceeds 1 MiB",
            ));
        }
        line.push(b'\n');
        self.process
            .write_stdin(&line)
            .map_err(|_| self.disconnected())
    }

    fn response(&mut self, id: i64, cancel: &CancelToken) -> Result<Value, McpError> {
        loop {
            if let Some(error) = cancelled_error(cancel, &self.shared) {
                return Err(error);
            }
            let wait = cancel.remaining().map_or(POLL, |left| left.min(POLL));
            match self
                .responses
                .recv_timeout(wait.max(Duration::from_millis(1)))
            {
                Ok(Ok((response_id, outcome))) if id_matches(&response_id, id) => return outcome,
                Ok(Ok(_)) => {}
                Ok(Err(error)) => return Err(error),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(self.disconnected()),
            }
        }
    }

    fn is_alive(&self) -> bool {
        self.shared.is_alive() && self.process.is_running()
    }

    fn close(&mut self) {
        self.shared.close();
        self.process.close_stdin();
        if matches!(self.process.wait_timeout(GRACEFUL_EXIT), Ok(None)) {
            let _ = self.process.kill_tree();
            let _ = self.process.wait_timeout(Duration::from_secs(5));
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for StdioChannel {
    fn drop(&mut self) {
        if self.reader.is_some() {
            self.shared.close();
            let _ = self.process.kill_tree();
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }
}

fn read_loop(
    process: &Arc<ManagedProcess>,
    shared: &Arc<Shared>,
    scrubber: &Scrubber,
    sender: &Sender<Delivery>,
) {
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let running = process.is_running();
        let output = process.take_output();
        shared.logs.push_stderr(&output.stderr.bytes);
        if output.stdout.truncated {
            return fatal(
                process,
                shared,
                sender,
                McpError::new(
                    McpErrorKind::OutputLimit,
                    "the MCP server wrote more output than AiDEX could buffer",
                ),
            );
        }
        pending.extend_from_slice(&output.stdout.bytes);
        while let Some(end) = pending.iter().position(|b| *b == b'\n') {
            if end > MAX_FRAME {
                return fatal(process, shared, sender, frame_too_large());
            }
            let line: Vec<u8> = pending.drain(..=end).collect();
            let line = &line[..line.len() - 1];
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let Ok(frame) = serde_json::from_slice::<Value>(line) else {
                shared.logs.push(
                    LogStream::Client,
                    &format!(
                        "ignored a non-JSON line on stdout: {}",
                        bounded_redacted(&scrubber.text(&String::from_utf8_lossy(line)), 200)
                    ),
                );
                continue;
            };
            match route(shared, scrubber.value(frame)) {
                Ok(Routed::Response { id, outcome }) => {
                    if sender.send(Ok((id, outcome))).is_err() {
                        return;
                    }
                }
                Ok(Routed::Reply(reply)) => {
                    if let Ok(mut bytes) = serde_json::to_vec(&reply) {
                        bytes.push(b'\n');
                        let _ = process.write_stdin(&bytes);
                    }
                }
                Ok(Routed::Handled) => {}
                Err(error) => shared.logs.push(
                    LogStream::Client,
                    &format!("ignored an invalid message: {}", error.message),
                ),
            }
        }
        if pending.len() > MAX_FRAME {
            return fatal(process, shared, sender, frame_too_large());
        }
        if !running {
            shared.logs.flush_stderr();
            if !shared.is_closed() {
                let code = process
                    .try_wait()
                    .ok()
                    .flatten()
                    .and_then(|status| status.code);
                let detail = shared
                    .logs
                    .last_stderr()
                    .map(|line| format!(": {line}"))
                    .unwrap_or_default();
                let error = McpError::new(
                    McpErrorKind::Disconnected,
                    match code {
                        Some(code) => format!("the MCP server exited with code {code}{detail}"),
                        None => format!("the MCP server exited{detail}"),
                    },
                );
                shared.logs.push(LogStream::Client, &error.message);
                shared.end(error.clone());
                let _ = sender.send(Err(error));
            }
            return;
        }
        if shared.is_closed() {
            return;
        }
        thread::sleep(POLL);
    }
}

fn frame_too_large() -> McpError {
    McpError::new(
        McpErrorKind::OutputLimit,
        "the MCP server sent a frame larger than 1 MiB",
    )
}

fn fatal(process: &ManagedProcess, shared: &Shared, sender: &Sender<Delivery>, error: McpError) {
    shared.logs.push(LogStream::Client, &error.message);
    shared.end(error.clone());
    let _ = process.kill_tree();
    let _ = sender.send(Err(error));
}
