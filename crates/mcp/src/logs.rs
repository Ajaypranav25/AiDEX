//! A bounded ring buffer of one server's log lines: its stderr, the log
//! notifications it sends, and the client's own lifecycle notes. Every line is
//! scrubbed of the server's secret values and credential patterns.

use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError},
};

use aidex_core::Timestamp;
use serde::Serialize;

use crate::redact::{Scrubber, bounded_redacted};

/// Lines kept per server.
pub const MAX_LOG_LINES: usize = 400;
/// Bytes kept per server.
const MAX_LOG_BYTES: usize = 128 * 1024;
/// Longest line kept; longer lines are cut.
const MAX_LINE_BYTES: usize = 2048;

/// Where a log line came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    /// The server process's standard error.
    Stderr,
    /// A `notifications/message` log notification from the server.
    Server,
    /// AiDEX's own notes (started, stopped, failures).
    Client,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LogLine {
    pub at: Timestamp,
    pub stream: LogStream,
    pub text: String,
}

/// The newest lines, oldest first.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LogTail {
    pub lines: Vec<LogLine>,
    /// Lines that were evicted from the buffer so far.
    pub dropped: u64,
}

#[derive(Default)]
struct State {
    lines: VecDeque<LogLine>,
    bytes: usize,
    dropped: u64,
    partial: Vec<u8>,
    scrubber: Scrubber,
}

/// One server's log buffer. Shared between the supervisor and its connection.
#[derive(Default)]
pub struct LogBuffer {
    state: Mutex<State>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Scrub these secret values from every later line.
    pub fn set_scrubber(&self, scrubber: Scrubber) {
        self.state().scrubber = scrubber;
    }

    /// Append raw stderr bytes; complete lines are recorded, the rest waits.
    pub fn push_stderr(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut state = self.state();
        state.partial.extend_from_slice(bytes);
        while let Some(end) = state.partial.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = state.partial.drain(..=end).collect();
            let text = String::from_utf8_lossy(&line[..line.len() - 1]).into_owned();
            record(&mut state, LogStream::Stderr, &text);
        }
        if state.partial.len() > MAX_LINE_BYTES {
            let line = std::mem::take(&mut state.partial);
            record(
                &mut state,
                LogStream::Stderr,
                &String::from_utf8_lossy(&line),
            );
        }
    }

    /// Record any stderr text still waiting for a newline.
    pub fn flush_stderr(&self) {
        let mut state = self.state();
        if !state.partial.is_empty() {
            let line = std::mem::take(&mut state.partial);
            record(
                &mut state,
                LogStream::Stderr,
                &String::from_utf8_lossy(&line),
            );
        }
    }

    /// Record `text` (one line per newline-separated part).
    pub fn push(&self, stream: LogStream, text: &str) {
        let mut state = self.state();
        for line in text.lines() {
            record(&mut state, stream, line);
        }
    }

    /// The newest `limit` lines.
    pub fn tail(&self, limit: usize) -> LogTail {
        let state = self.state();
        let skip = state.lines.len().saturating_sub(limit);
        LogTail {
            lines: state.lines.iter().skip(skip).cloned().collect(),
            dropped: state.dropped + skip as u64,
        }
    }

    /// The newest stderr line, for failure messages.
    pub fn last_stderr(&self) -> Option<String> {
        self.state()
            .lines
            .iter()
            .rev()
            .find(|line| line.stream == LogStream::Stderr)
            .map(|line| line.text.clone())
    }
}

fn record(state: &mut State, stream: LogStream, text: &str) {
    let text = text.trim_end_matches('\r');
    if text.trim().is_empty() {
        return;
    }
    let scrubbed = state.scrubber.text(text);
    let text = bounded_redacted(&scrubbed, MAX_LINE_BYTES);
    state.bytes += text.len();
    state.lines.push_back(LogLine {
        at: Timestamp::now(),
        stream,
        text,
    });
    while state.lines.len() > MAX_LOG_LINES || state.bytes > MAX_LOG_BYTES {
        match state.lines.pop_front() {
            Some(old) => {
                state.bytes -= old.text.len();
                state.dropped += 1;
            }
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    #[test]
    fn stderr_is_split_scrubbed_and_bounded() {
        let logs = LogBuffer::new();
        logs.set_scrubber(Scrubber::new(vec![Zeroizing::new("value-9876".into())]));
        logs.push_stderr(b"first line\r\nsecret value-9876 here\npart");
        logs.push_stderr(b"ial\n");
        let tail = logs.tail(10);
        let texts: Vec<&str> = tail.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["first line", "secret [REDACTED] here", "partial"]);
        assert_eq!(logs.last_stderr().as_deref(), Some("partial"));
        for n in 0..(MAX_LOG_LINES + 10) {
            logs.push(LogStream::Client, &format!("line {n}"));
        }
        let tail = logs.tail(5);
        assert_eq!(tail.lines.len(), 5);
        assert!(tail.dropped > 0);
        assert_eq!(
            tail.lines.last().unwrap().text,
            format!("line {}", MAX_LOG_LINES + 9)
        );
    }
}
