//! Subscription streams: a [`NotificationSink`] pushes notifications on one
//! connection until the client unsubscribes, the connection closes, or the
//! connection's bounded outbound queue overflows.
//!
//! Every stream notification has the envelope
//! `{"jsonrpc":"2.0","method":"<topic>","params":{"subscription":"<id>","data":<payload>}}`.
//! The subscribe response is always written before the first notification:
//! until the subscribe handler returns, other threads' sends wait, and sends
//! from the handler's own thread are held back (bounded by the queue limit).

use crate::{
    rpc::{Message, RESERVED_PREFIX, UNSUBSCRIBED_NOTIFICATION, is_valid_method_name},
    transport::{ConnectionShared, PushError},
};
use aidex_core::ServiceErrorCode;
use serde_json::{Value, json};
use std::{
    fmt,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::{self, ThreadId},
};

enum Phase {
    /// The subscribe handler has not returned; notifications are held back.
    Pending {
        buffered: Vec<String>,
        ended: bool,
    },
    Active,
    Closed,
}

pub(crate) struct SubscriptionState {
    id: String,
    phase: Mutex<Phase>,
    settled: Condvar,
    /// The connection reader thread that runs the subscribe handler.
    owner: ThreadId,
}

impl SubscriptionState {
    /// Create on the thread that will run the subscribe handler.
    pub(crate) fn new(id: String) -> Self {
        Self {
            id,
            phase: Mutex::new(Phase::Pending {
                buffered: Vec::new(),
                ended: false,
            }),
            settled: Condvar::new(),
            owner: thread::current().id(),
        }
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    fn lock(&self) -> MutexGuard<'_, Phase> {
        self.phase
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Mark closed; later sends fail. Callers must not hold another phase lock.
    pub(crate) fn close(&self) {
        *self.lock() = Phase::Closed;
        self.settled.notify_all();
    }

    fn is_closed(&self) -> bool {
        matches!(*self.lock(), Phase::Closed)
    }

    /// Write the subscribe response, then any held-back notifications.
    pub(crate) fn activate(&self, connection: &ConnectionShared, response: String) {
        let mut overflow = false;
        let finished;
        {
            let mut phase = self.lock();
            match std::mem::replace(&mut *phase, Phase::Active) {
                Phase::Pending { buffered, ended } => {
                    overflow = connection.enqueue(response, true) == Err(PushError::Full);
                    for line in buffered {
                        if overflow {
                            break;
                        }
                        overflow = connection.enqueue(line, false) == Err(PushError::Full);
                    }
                    if ended && !overflow {
                        overflow = connection.enqueue(unsubscribed_line(&self.id), false)
                            == Err(PushError::Full);
                    }
                    finished = ended || overflow;
                }
                Phase::Active | Phase::Closed => {
                    let _ = connection.enqueue(response, true);
                    finished = true;
                }
            }
            if finished {
                *phase = Phase::Closed;
            }
        }
        self.settled.notify_all();
        if overflow {
            connection.overflowed();
        }
        if finished {
            connection.remove_subscription(&self.id);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkError {
    /// The client unsubscribed or the connection closed.
    Closed,
    /// The client stopped draining its queue; the connection was dropped.
    Overflow,
    /// The encoded notification exceeds the message limit; nothing was sent.
    TooLarge { bytes: usize, limit: usize },
    /// Topics must be `<domain>.<event>` names outside the `aidex.` prefix.
    InvalidMethod(String),
}

impl SinkError {
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Closed | Self::Overflow => ServiceErrorCode::Unavailable,
            Self::TooLarge { .. } | Self::InvalidMethod(_) => ServiceErrorCode::InvalidArgument,
        }
    }
}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("subscription is closed"),
            Self::Overflow => f.write_str("subscriber stopped reading; connection dropped"),
            Self::TooLarge { bytes, limit } => {
                write!(
                    f,
                    "notification of {bytes} bytes exceeds the {limit}-byte limit"
                )
            }
            Self::InvalidMethod(method) => write!(f, "invalid notification method `{method}`"),
        }
    }
}

impl std::error::Error for SinkError {}

/// Pushes notifications for one subscription. Move it into a thread; drop it
/// to end the stream (the client then receives `aidex.unsubscribed`).
pub struct NotificationSink {
    state: Arc<SubscriptionState>,
    connection: Arc<ConnectionShared>,
}

impl NotificationSink {
    pub(crate) fn new(state: Arc<SubscriptionState>, connection: Arc<ConnectionShared>) -> Self {
        Self { state, connection }
    }

    pub fn subscription_id(&self) -> &str {
        &self.state.id
    }

    pub fn connection_id(&self) -> u64 {
        self.connection.id()
    }

    /// Whether further sends will fail. Poll it while waiting on a quiet source.
    pub fn is_closed(&self) -> bool {
        self.state.is_closed() || !self.connection.is_open()
    }

    /// Queue `{"method": method, "params": {"subscription", "data"}}` without blocking.
    pub fn notify(&self, method: &str, data: Value) -> Result<(), SinkError> {
        if method.starts_with(RESERVED_PREFIX) || !is_valid_method_name(method) {
            return Err(SinkError::InvalidMethod(method.to_owned()));
        }
        let line = Message::notification(
            method,
            json!({ "subscription": self.state.id, "data": data }),
        )
        .encode_line();
        let limit = self.connection.max_message_bytes();
        if line.len() - 1 > limit {
            return Err(SinkError::TooLarge {
                bytes: line.len() - 1,
                limit,
            });
        }
        let mut overflow = false;
        {
            let mut phase = self.state.lock();
            let foreign = thread::current().id() != self.state.owner;
            while foreign && matches!(*phase, Phase::Pending { .. }) {
                phase = self
                    .state
                    .settled
                    .wait(phase)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            match &mut *phase {
                Phase::Closed => return Err(SinkError::Closed),
                Phase::Pending { buffered, .. } => {
                    if buffered.len() < self.connection.max_queued_messages() {
                        buffered.push(line);
                        return Ok(());
                    }
                    overflow = true;
                }
                Phase::Active => match self.connection.enqueue(line, false) {
                    Ok(()) => return Ok(()),
                    Err(PushError::Closed) => {}
                    Err(PushError::Full) => overflow = true,
                },
            }
            *phase = Phase::Closed;
        }
        if overflow {
            self.connection.overflowed();
            Err(SinkError::Overflow)
        } else {
            Err(SinkError::Closed)
        }
    }
}

impl Drop for NotificationSink {
    fn drop(&mut self) {
        let mut ended = false;
        {
            let mut phase = self.state.lock();
            match &mut *phase {
                Phase::Pending { ended: flag, .. } => *flag = true,
                Phase::Active => {
                    *phase = Phase::Closed;
                    ended = true;
                }
                Phase::Closed => {}
            }
        }
        if ended {
            if self
                .connection
                .enqueue(unsubscribed_line(&self.state.id), false)
                == Err(PushError::Full)
            {
                self.connection.overflowed();
            }
            self.connection.remove_subscription(&self.state.id);
        }
    }
}

impl fmt::Debug for NotificationSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NotificationSink")
            .field("subscription", &self.state.id)
            .field("connection", &self.connection.id())
            .finish()
    }
}

fn unsubscribed_line(id: &str) -> String {
    Message::notification(
        UNSUBSCRIBED_NOTIFICATION,
        json!({ "subscription": id, "reason": "completed" }),
    )
    .encode_line()
}
