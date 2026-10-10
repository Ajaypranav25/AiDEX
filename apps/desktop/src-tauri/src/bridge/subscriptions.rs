//! Daemon stream subscriptions forwarded to the UI as Tauri events.
//!
//! A `Subscription` borrows its client for its whole life, so every subscription gets
//! its own connection and its own worker thread. The worker polls
//! `Subscription::next_timeout`, forwards each notification through a
//! [`NotificationEmitter`], and exits when the stream ends, the connection closes, or
//! the UI unsubscribes. Live subscriptions are capped at [`MAX_SUBSCRIPTIONS`] and all
//! of them are dropped when the window reloads or closes.

use crate::error::{kind, BridgeError};
use aidex_core::ServiceErrorCode;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

/// Most live subscriptions per app (the daemon allows 64 connections per drive).
pub const MAX_SUBSCRIPTIONS: usize = 16;
/// How often a worker checks for an unsubscribe request between notifications.
pub const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// How long `subscribe` waits for the daemon to accept the stream.
pub const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(30);

/// Tauri event carrying one stream notification.
pub const NOTIFICATION_EVENT: &str = "aidex://notification";
/// Tauri event sent once when a subscription ends for any reason but `unsubscribe`.
pub const ENDED_EVENT: &str = "aidex://subscription-ended";

/// Payload of [`NOTIFICATION_EVENT`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NotificationPayload {
    /// Bridge-local subscription id returned by `rpc_subscribe`.
    pub subscription: String,
    /// Notification method, e.g. `events.event`.
    pub method: String,
    /// The stream's `data` payload.
    pub data: Value,
}

/// Payload of [`ENDED_EVENT`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EndedPayload {
    pub subscription: String,
    /// The daemon's reason (`completed`, `shutting_down`, ...) or why the connection failed.
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BridgeError>,
}

/// Where workers deliver notifications (the Tauri app handle in production).
pub trait NotificationEmitter: Send + Sync + 'static {
    fn notification(&self, payload: NotificationPayload);
    fn ended(&self, payload: EndedPayload);
}

/// Result of a successful subscribe.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Subscribed {
    pub subscription: String,
    /// The `data` the daemon's subscribe handler returned (for example a cursor).
    pub initial: Value,
}

#[derive(Default)]
struct Registry {
    next_id: u64,
    active: HashMap<String, Arc<AtomicBool>>,
}

/// Live subscriptions of this app.
#[derive(Clone, Default)]
pub struct Subscriptions {
    registry: Arc<Mutex<Registry>>,
}

impl Subscriptions {
    /// Open a dedicated connection, subscribe, and start forwarding notifications.
    pub fn subscribe(
        &self,
        root: &Path,
        method: &str,
        params: Value,
        emitter: Arc<dyn NotificationEmitter>,
    ) -> Result<Subscribed, BridgeError> {
        super::connection::validate_method(method)?;
        let (id, stop) = self.reserve()?;
        let (ready_tx, ready_rx) = sync_channel::<Result<Value, BridgeError>>(1);
        let worker = Worker {
            id: id.clone(),
            root: root.to_path_buf(),
            method: method.to_owned(),
            stop,
            registry: self.clone(),
            emitter,
        };
        let spawned = thread::Builder::new()
            .name(format!("aidex-{id}"))
            .spawn(move || worker.run(params, ready_tx));
        if let Err(error) = spawned {
            self.release(&id);
            return Err(BridgeError::internal(format!(
                "cannot start a subscription worker: {error}"
            )));
        }
        let outcome = match ready_rx.recv_timeout(SUBSCRIBE_TIMEOUT) {
            Ok(outcome) => outcome,
            Err(RecvTimeoutError::Timeout) => Err(BridgeError::new(
                ServiceErrorCode::Unavailable,
                kind::DAEMON_UNAVAILABLE,
                format!(
                    "`{method}` did not answer within {} s",
                    SUBSCRIBE_TIMEOUT.as_secs()
                ),
            )),
            Err(RecvTimeoutError::Disconnected) => Err(BridgeError::internal(
                "subscription worker exited unexpectedly",
            )),
        };
        match outcome {
            Ok(initial) => Ok(Subscribed {
                subscription: id,
                initial,
            }),
            Err(error) => {
                // A late success after a timeout is stopped by the cleared flag.
                self.unsubscribe(&id);
                Err(error)
            }
        }
    }

    /// Stop a subscription. Returns whether it was live. The worker unsubscribes on
    /// the daemon within one [`POLL_INTERVAL`] and emits no further events.
    pub fn unsubscribe(&self, id: &str) -> bool {
        let flag = self
            .registry
            .lock()
            .ok()
            .and_then(|mut registry| registry.active.remove(id));
        flag.inspect(|flag| flag.store(true, Ordering::SeqCst))
            .is_some()
    }

    /// Stop every subscription (window reload or close, drive change, exit).
    pub fn clear(&self) -> usize {
        let flags: Vec<_> = self
            .registry
            .lock()
            .map(|mut registry| registry.active.drain().map(|(_, flag)| flag).collect())
            .unwrap_or_default();
        for flag in &flags {
            flag.store(true, Ordering::SeqCst);
        }
        flags.len()
    }

    /// Number of live subscriptions.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.registry
            .lock()
            .map(|registry| registry.active.len())
            .unwrap_or(0)
    }

    fn reserve(&self) -> Result<(String, Arc<AtomicBool>), BridgeError> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| BridgeError::internal("subscription registry lock poisoned"))?;
        if registry.active.len() >= MAX_SUBSCRIPTIONS {
            return Err(BridgeError::new(
                ServiceErrorCode::Unavailable,
                kind::SUBSCRIPTION_LIMIT,
                format!("at most {MAX_SUBSCRIPTIONS} subscriptions can be open at once"),
            ));
        }
        registry.next_id += 1;
        let id = format!("sub-{}", registry.next_id);
        let stop = Arc::new(AtomicBool::new(false));
        registry.active.insert(id.clone(), Arc::clone(&stop));
        Ok((id, stop))
    }

    /// Forget `id` without signalling (the worker is already exiting).
    fn release(&self, id: &str) -> bool {
        self.registry
            .lock()
            .is_ok_and(|mut registry| registry.active.remove(id).is_some())
    }
}

struct Worker {
    id: String,
    root: PathBuf,
    method: String,
    stop: Arc<AtomicBool>,
    registry: Subscriptions,
    emitter: Arc<dyn NotificationEmitter>,
}

impl Worker {
    fn run(self, params: Value, ready: std::sync::mpsc::SyncSender<Result<Value, BridgeError>>) {
        let mut client = match super::connection::connect(&self.root) {
            Ok(client) => client,
            Err(error) => {
                self.registry.release(&self.id);
                let _ = ready.send(Err(error));
                return;
            }
        };
        let mut subscription = match client.subscribe(&self.method, params) {
            Ok(subscription) => subscription,
            Err(error) => {
                self.registry.release(&self.id);
                let _ = ready.send(Err(error.into()));
                return;
            }
        };
        let _ = ready.send(Ok(subscription.initial().clone()));
        let ended = loop {
            if self.stop.load(Ordering::SeqCst) {
                let _ = subscription.unsubscribe();
                return;
            }
            match subscription.next_timeout(POLL_INTERVAL) {
                Ok(Some(notification)) => {
                    if self.stop.load(Ordering::SeqCst) {
                        continue;
                    }
                    self.emitter.notification(NotificationPayload {
                        subscription: self.id.clone(),
                        method: notification.method,
                        data: notification.params,
                    });
                }
                Ok(None) if subscription.is_ended() => {
                    let reason = subscription.end_reason().unwrap_or("completed").to_owned();
                    break EndedPayload {
                        subscription: self.id.clone(),
                        reason,
                        error: None,
                    };
                }
                Ok(None) => {}
                Err(error) => {
                    break EndedPayload {
                        subscription: self.id.clone(),
                        reason: "connection closed".into(),
                        error: Some(error.into()),
                    };
                }
            }
        };
        // Only a subscription the UI still holds reports its end.
        if self.registry.release(&self.id) {
            self.emitter.ended(ended);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        notifications: Mutex<Vec<NotificationPayload>>,
        ended: Mutex<Vec<EndedPayload>>,
    }

    impl NotificationEmitter for Recorder {
        fn notification(&self, payload: NotificationPayload) {
            self.notifications.lock().unwrap().push(payload);
        }
        fn ended(&self, payload: EndedPayload) {
            self.ended.lock().unwrap().push(payload);
        }
    }

    #[test]
    fn reservations_are_capped_unique_and_released_by_unsubscribe() {
        let subscriptions = Subscriptions::default();
        let mut ids = Vec::new();
        for _ in 0..MAX_SUBSCRIPTIONS {
            ids.push(subscriptions.reserve().unwrap().0);
        }
        assert_eq!(subscriptions.len(), MAX_SUBSCRIPTIONS);
        let error = subscriptions.reserve().unwrap_err();
        assert_eq!(error.kind, kind::SUBSCRIPTION_LIMIT);

        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());

        assert!(subscriptions.unsubscribe(&ids[0]));
        assert!(!subscriptions.unsubscribe(&ids[0]), "already released");
        let (next, _) = subscriptions.reserve().unwrap();
        assert!(!ids.contains(&next), "ids are never reused");
        assert_eq!(subscriptions.clear(), MAX_SUBSCRIPTIONS);
        assert_eq!(subscriptions.len(), 0);
    }

    #[test]
    fn unsubscribe_and_clear_signal_the_worker_flags() {
        let subscriptions = Subscriptions::default();
        let (first, first_flag) = subscriptions.reserve().unwrap();
        let (_, second_flag) = subscriptions.reserve().unwrap();
        subscriptions.unsubscribe(&first);
        assert!(first_flag.load(Ordering::SeqCst));
        assert!(!second_flag.load(Ordering::SeqCst));
        subscriptions.clear();
        assert!(second_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn subscribing_without_a_daemon_fails_and_frees_the_slot() {
        let root = tempfile::tempdir().unwrap();
        let subscriptions = Subscriptions::default();
        let recorder = Arc::new(Recorder::default());
        let error = subscriptions
            .subscribe(
                root.path(),
                "events.subscribe",
                Value::Null,
                recorder.clone(),
            )
            .unwrap_err();
        assert_eq!(error.kind, kind::DAEMON_UNAVAILABLE);
        assert_eq!(subscriptions.len(), 0);
        assert!(recorder.ended.lock().unwrap().is_empty());

        let invalid = subscriptions
            .subscribe(root.path(), "aidex.unsubscribe", Value::Null, recorder)
            .unwrap_err();
        assert_eq!(invalid.kind, kind::INVALID_METHOD);
    }
}
