//! Bounded per-connection outbound queue drained by the writer thread.
//!
//! Producers never block: a full queue is reported to the caller, which closes
//! the connection as a slow consumer. The final `aidex.closing` notification
//! may exceed the limits so it can still be attempted.

use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex, MutexGuard},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    /// Accepting requests and notifications.
    Open,
    /// Draining: responses to in-flight requests may still be queued.
    Closing,
    /// The writer has exited; nothing more will be written.
    Closed,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PushError {
    Closed,
    Full,
}

pub(crate) enum Pop {
    Line(String),
    Finished,
}

struct State {
    queue: VecDeque<String>,
    bytes: usize,
    phase: Phase,
    in_flight: usize,
}

pub(crate) struct Outbox {
    state: Mutex<State>,
    changed: Condvar,
    max_messages: usize,
    max_bytes: usize,
}

impl Outbox {
    pub(crate) fn new(max_messages: usize, max_bytes: usize) -> Self {
        Self {
            state: Mutex::new(State {
                queue: VecDeque::new(),
                bytes: 0,
                phase: Phase::Open,
                in_flight: 0,
            }),
            changed: Condvar::new(),
            max_messages: max_messages.max(1),
            max_bytes: max_bytes.max(1),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A poisoned lock only means another thread panicked mid-update; the
        // queue itself stays structurally valid, so keep serving.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Queue a line. `response` lines are still accepted while draining.
    pub(crate) fn push(&self, line: String, response: bool) -> Result<(), PushError> {
        let mut state = self.lock();
        match state.phase {
            Phase::Open => {}
            Phase::Closing if response => {}
            _ => return Err(PushError::Closed),
        }
        if state.queue.len() >= self.max_messages || state.bytes + line.len() > self.max_bytes {
            return Err(PushError::Full);
        }
        state.bytes += line.len();
        state.queue.push_back(line);
        drop(state);
        self.changed.notify_all();
        Ok(())
    }

    /// Stop accepting new work and, on the first call only, queue
    /// `final_line` regardless of limits. Both happen under one lock so the
    /// writer cannot finish draining in between. Returns `true` on the first call.
    pub(crate) fn begin_close(&self, final_line: Option<String>) -> bool {
        let mut state = self.lock();
        let first = state.phase == Phase::Open;
        if first {
            state.phase = Phase::Closing;
            if let Some(line) = final_line {
                state.bytes += line.len();
                state.queue.push_back(line);
            }
        }
        drop(state);
        self.changed.notify_all();
        first
    }

    pub(crate) fn is_open(&self) -> bool {
        self.lock().phase == Phase::Open
    }

    /// Track a request being handled so its response is drained before close.
    pub(crate) fn begin_request(&self) -> bool {
        let mut state = self.lock();
        if state.phase != Phase::Open {
            return false;
        }
        state.in_flight += 1;
        true
    }

    pub(crate) fn end_request(&self) {
        let mut state = self.lock();
        state.in_flight = state.in_flight.saturating_sub(1);
        drop(state);
        self.changed.notify_all();
    }

    /// Block until a line is ready, or until draining has finished.
    pub(crate) fn pop(&self) -> Pop {
        let mut state = self.lock();
        loop {
            if let Some(line) = state.queue.pop_front() {
                state.bytes -= line.len();
                return Pop::Line(line);
            }
            if state.phase != Phase::Open && state.in_flight == 0 {
                return Pop::Finished;
            }
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// The writer has exited: drop anything still queued.
    pub(crate) fn finish(&self) {
        let mut state = self.lock();
        state.phase = Phase::Closed;
        state.queue.clear();
        state.bytes = 0;
        drop(state);
        self.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> String {
        format!("{text}\n")
    }

    #[test]
    fn rejects_beyond_message_and_byte_limits() {
        let outbox = Outbox::new(2, 1024);
        outbox.push(line("a"), false).unwrap();
        outbox.push(line("b"), false).unwrap();
        assert_eq!(outbox.push(line("c"), false), Err(PushError::Full));
        let bytes = Outbox::new(100, 8);
        bytes.push(line("1234"), false).unwrap();
        assert_eq!(bytes.push(line("5678"), false), Err(PushError::Full));
    }

    #[test]
    fn draining_accepts_responses_and_one_final_line_then_finishes() {
        let outbox = Outbox::new(1, 1024);
        outbox.push(line("queued"), false).unwrap();
        assert!(outbox.begin_request());
        assert!(outbox.begin_close(Some(line("closing"))));
        assert!(!outbox.begin_close(Some(line("ignored"))));
        assert!(!outbox.begin_request());
        assert_eq!(outbox.push(line("event"), false), Err(PushError::Closed));
        assert!(matches!(outbox.pop(), Pop::Line(l) if l == "queued\n"));
        assert!(matches!(outbox.pop(), Pop::Line(l) if l == "closing\n"));
        outbox.push(line("response"), true).unwrap();
        outbox.end_request();
        assert!(matches!(outbox.pop(), Pop::Line(l) if l == "response\n"));
        assert!(matches!(outbox.pop(), Pop::Finished));
        outbox.finish();
        assert_eq!(outbox.push(line("late"), true), Err(PushError::Closed));
    }
}
