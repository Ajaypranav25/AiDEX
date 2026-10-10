//! Loopback listener, connection registry, and orderly shutdown.
//!
//! Each accepted connection gets a reader thread (framing, handshake,
//! dispatch) and a writer thread draining a bounded outbox, so one slow or
//! stalled client never blocks another. See `transport/README.md`.

mod connection;
mod outbox;

pub(crate) use connection::ConnectionShared;
pub(crate) use outbox::PushError;

use crate::{
    endpoint::Capability,
    errors::{ErrorCode, RpcError},
    lifecycle::{DriveProbe, ShutdownSignal, StopReason},
    router::Router,
    rpc::{Id, error_line},
};
use std::{
    collections::HashMap,
    io::{self, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// Transport limits. Defaults are the contract-version-2 production values.
#[derive(Clone, Debug)]
pub struct TransportConfig {
    /// Concurrent connections, including ones still handshaking.
    pub max_connections: usize,
    /// Largest accepted or emitted message line, excluding the newline.
    pub max_message_bytes: usize,
    /// Outbound messages queued per connection before it is dropped as slow.
    pub max_queued_messages: usize,
    /// Outbound bytes queued per connection before it is dropped as slow.
    pub max_queued_bytes: usize,
    /// Concurrent subscriptions per connection.
    pub max_subscriptions: usize,
    /// Time allowed between accept and a complete handshake line.
    pub handshake_timeout: Duration,
    /// A single blocked socket write longer than this drops the connection.
    pub write_timeout: Duration,
    /// How long shutdown waits for connections to drain before forcing them closed.
    pub shutdown_grace: Duration,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            max_connections: 64,
            max_message_bytes: crate::rpc::MAX_MESSAGE_BYTES,
            max_queued_messages: 1024,
            max_queued_bytes: 16 * 1024 * 1024,
            max_subscriptions: 64,
            handshake_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(10),
            shutdown_grace: Duration::from_secs(5),
        }
    }
}

pub(crate) struct ServerShared {
    pub(crate) router: Router,
    pub(crate) token: Capability,
    pub(crate) config: TransportConfig,
    pub(crate) shutdown: ShutdownSignal,
    connections: Mutex<HashMap<u64, Arc<ConnectionShared>>>,
    idle: Condvar,
    next_id: AtomicU64,
}

impl ServerShared {
    fn connections(&self) -> MutexGuard<'_, HashMap<u64, Arc<ConnectionShared>>> {
        self.connections
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn next_connection_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(crate) fn insert(&self, connection: Arc<ConnectionShared>) {
        self.connections().insert(connection.id(), connection);
    }

    pub(crate) fn remove(&self, id: u64) {
        self.connections().remove(&id);
        self.idle.notify_all();
    }

    fn snapshot(&self) -> Vec<Arc<ConnectionShared>> {
        self.connections().values().cloned().collect()
    }

    fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut connections = self.connections();
        while !connections.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            connections = self
                .idle
                .wait_timeout(connections, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
        true
    }
}

pub(crate) struct Server {
    listener: TcpListener,
    shared: Arc<ServerShared>,
}

impl Server {
    /// Bind an ephemeral IPv4 loopback port. Never binds a routable address.
    pub(crate) fn bind(
        router: Router,
        token: Capability,
        config: TransportConfig,
        shutdown: ShutdownSignal,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            shared: Arc::new(ServerShared {
                router,
                token,
                config,
                shutdown,
                connections: Mutex::new(HashMap::new()),
                idle: Condvar::new(),
                next_id: AtomicU64::new(0),
            }),
        })
    }

    pub(crate) fn port(&self) -> io::Result<u16> {
        Ok(self.listener.local_addr()?.port())
    }

    /// Accept until shutdown is requested or `root` disappears, run `stopping`,
    /// then close every connection with an `aidex.closing` notification.
    pub(crate) fn run(
        self,
        root: &Path,
        poll: Duration,
        drive_present: &DriveProbe,
        stopping: impl FnOnce(&io::Result<StopReason>),
    ) -> io::Result<StopReason> {
        let outcome = self.accept_loop(root, poll, drive_present);
        drop(self.listener);
        stopping(&outcome);
        let message = match &outcome {
            Ok(StopReason::DriveRemoved) => "the selected drive was removed; daemon is stopping",
            _ => "daemon is stopping",
        };
        let reason = RpcError::new(ErrorCode::ShuttingDown, message);
        for connection in self.shared.snapshot() {
            connection.close(Some(reason.clone()));
        }
        if !self.shared.wait_idle(self.shared.config.shutdown_grace) {
            for connection in self.shared.snapshot() {
                connection.shutdown_socket();
            }
            self.shared.wait_idle(Duration::from_secs(1));
        }
        outcome
    }

    fn accept_loop(
        &self,
        root: &Path,
        poll: Duration,
        drive_present: &DriveProbe,
    ) -> io::Result<StopReason> {
        loop {
            if self.shared.shutdown.is_requested() {
                return Ok(StopReason::Requested);
            }
            if !drive_present(root) {
                return Ok(StopReason::DriveRemoved);
            }
            match self.listener.accept() {
                Ok((stream, peer)) if peer.ip().is_loopback() => self.admit(stream),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(poll),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::ConnectionReset
                    ) => {}
                Err(error) => return Err(error),
            }
        }
    }

    fn admit(&self, stream: TcpStream) {
        // Accepted sockets inherit non-blocking mode on some platforms.
        if stream.set_nonblocking(false).is_err() {
            return;
        }
        let _ = stream.set_nodelay(true);
        if self.shared.connections().len() >= self.shared.config.max_connections {
            reject_busy(stream);
            return;
        }
        // A failed thread spawn has already cleaned up; the client sees a closed socket.
        let _ = connection::spawn(stream, &self.shared);
    }
}

fn reject_busy(mut stream: TcpStream) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let line = error_line(
        Id::Null,
        ErrorCode::ServerBusy,
        "daemon connection limit reached; retry later",
    );
    let _ = stream.write_all(line.as_bytes());
    let _ = stream.shutdown(Shutdown::Write);
}
