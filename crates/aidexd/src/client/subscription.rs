//! Stream subscriptions over a [`Client`].

use super::{Client, ClientError};
use crate::{
    errors::ErrorCode,
    rpc::{Message, UNSUBSCRIBE_METHOD, UNSUBSCRIBED_NOTIFICATION},
};
use serde_json::{Value, json};
use std::{
    fmt,
    time::{Duration, Instant},
};

/// A notification delivered to a [`Subscription`]. `params` is the stream's
/// `data` payload; for `aidex.unsubscribed` it is the raw params.
#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    pub method: String,
    pub subscription: Option<String>,
    pub params: Value,
}

/// A live subscription. Borrows the client; drop or [`Subscription::unsubscribe`] to end it.
pub struct Subscription<'a> {
    client: &'a mut Client,
    id: String,
    initial: Value,
    ended: Option<String>,
    released: bool,
}

impl<'a> Subscription<'a> {
    pub(super) fn new(client: &'a mut Client, id: String, initial: Value) -> Self {
        Self {
            client,
            id,
            initial,
            ended: None,
            released: false,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The `data` the subscribe handler returned.
    pub fn initial(&self) -> &Value {
        &self.initial
    }

    /// The daemon ended the stream or the connection closed.
    pub fn is_ended(&self) -> bool {
        self.ended.is_some()
    }

    pub fn end_reason(&self) -> Option<&str> {
        self.ended.as_deref()
    }

    /// Next notification; `Ok(None)` on timeout or once the stream has ended
    /// (check [`Self::is_ended`]).
    pub fn next_timeout(&mut self, timeout: Duration) -> Result<Option<Notification>, ClientError> {
        if self.ended.is_some() {
            return Ok(None);
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(notification) = self.client.take_buffered(&self.id) {
                if notification.method == UNSUBSCRIBED_NOTIFICATION {
                    let reason = notification
                        .params
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("completed");
                    self.ended = Some(reason.to_owned());
                    self.client.release(&self.id);
                    return Ok(None);
                }
                return Ok(Some(notification));
            }
            match self.client.recv(deadline) {
                Ok(None) => return Ok(None),
                Ok(Some(Message::Notification { method, params })) => {
                    if let Err(error) = self.client.accept_notification(method, params) {
                        self.ended = Some("protocol error".into());
                        return Err(error);
                    }
                }
                Ok(Some(Message::Response { .. })) => {}
                Ok(Some(Message::Request { .. })) => {
                    self.ended = Some("protocol error".into());
                    return Err(self.client.violation("daemon sent a request"));
                }
                Err(error) => {
                    self.ended = Some("connection closed".into());
                    return Err(error);
                }
            }
        }
    }

    /// End the subscription and wait for the daemon to confirm.
    pub fn unsubscribe(mut self) -> Result<(), ClientError> {
        self.released = true;
        self.client.release(&self.id);
        if self.ended.is_some() {
            return Ok(());
        }
        let params = json!({ "subscription": self.id });
        match self.client.call(UNSUBSCRIBE_METHOD, params) {
            Ok(_) => Ok(()),
            // The stream ended on the daemon side first.
            Err(error) if error.rpc_kind() == Some(ErrorCode::NotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl Iterator for Subscription<'_> {
    type Item = Result<Notification, ClientError>;

    /// Blocks until the next notification; `None` once the stream ends.
    fn next(&mut self) -> Option<Self::Item> {
        while self.ended.is_none() {
            match self.next_timeout(Duration::from_secs(3600)) {
                Ok(Some(notification)) => return Some(Ok(notification)),
                Ok(None) => {}
                Err(error) => return Some(Err(error)),
            }
        }
        None
    }
}

impl Drop for Subscription<'_> {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        self.client.release(&self.id);
        if self.ended.is_none() {
            let params = json!({ "subscription": self.id });
            let _ = self.client.notify(UNSUBSCRIBE_METHOD, params);
        }
    }
}

impl fmt::Debug for Subscription<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscription")
            .field("id", &self.id)
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}
