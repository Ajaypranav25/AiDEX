//! A synchronous in-process event broadcaster for cooperating components.

use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    pub sequence: u64,
    pub topic: String,
    pub payload: Vec<u8>,
}

/// Subscribers receive every event published after their subscription begins.
#[derive(Default)]
pub struct EventBus {
    next_sequence: AtomicU64,
    subscribers: Mutex<Vec<Sender<Event>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self) -> Receiver<Event> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(sender);
        receiver
    }

    /// Broadcast an event and return its monotonically assigned sequence number.
    pub fn publish(&self, topic: impl Into<String>, payload: impl Into<Vec<u8>>) -> u64 {
        let mut subscribers = self.subscribers.lock().unwrap_or_else(|e| e.into_inner());
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let event = Event {
            sequence,
            topic: topic.into(),
            payload: payload.into(),
        };
        subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
        sequence
    }
}
