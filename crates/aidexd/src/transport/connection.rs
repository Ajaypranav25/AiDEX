//! One authenticated connection: reader thread (framing, handshake, dispatch)
//! and writer thread (drains the bounded outbox).

use super::{
    ServerShared,
    outbox::{Outbox, Pop, PushError},
};
use crate::{
    errors::{ErrorCode, RpcError},
    framing::{Frame, LineReader},
    handshake::{self, HandshakeResult, MAX_HANDSHAKE_BYTES, ServerInfo},
    router::{CallContext, ClientInfo, Route, StreamHandler},
    rpc::{
        CLOSING_NOTIFICATION, CONTRACT_VERSION, HANDSHAKE_METHOD, Id, Malformed, Message,
        UNSUBSCRIBE_METHOD, parse_params, to_result,
    },
    stream::{NotificationSink, SubscriptionState},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// After a protocol violation, keep reading briefly so the peer can receive
/// the error line before the socket closes (avoids a TCP reset).
const LINGER: Duration = Duration::from_secs(1);
const LINGER_BYTES: usize = 4 * 1024 * 1024;

pub(crate) struct ConnectionShared {
    id: u64,
    outbox: Outbox,
    socket: TcpStream,
    subscriptions: Mutex<HashMap<String, Arc<SubscriptionState>>>,
    next_subscription: AtomicU64,
    reader_finished: AtomicBool,
    max_message_bytes: usize,
    max_queued_messages: usize,
    max_subscriptions: usize,
}

impl ConnectionShared {
    fn new(id: u64, socket: TcpStream, server: &ServerShared) -> Self {
        let config = &server.config;
        Self {
            id,
            outbox: Outbox::new(config.max_queued_messages, config.max_queued_bytes),
            socket,
            subscriptions: Mutex::new(HashMap::new()),
            next_subscription: AtomicU64::new(0),
            reader_finished: AtomicBool::new(false),
            max_message_bytes: config.max_message_bytes,
            max_queued_messages: config.max_queued_messages,
            max_subscriptions: config.max_subscriptions,
        }
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn is_open(&self) -> bool {
        self.outbox.is_open()
    }

    pub(crate) fn max_message_bytes(&self) -> usize {
        self.max_message_bytes
    }

    pub(crate) fn max_queued_messages(&self) -> usize {
        self.max_queued_messages
    }

    fn subscriptions(&self) -> MutexGuard<'_, HashMap<String, Arc<SubscriptionState>>> {
        self.subscriptions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Queue a line without blocking. The caller handles `Full` via [`Self::overflowed`]
    /// after releasing any subscription lock.
    pub(crate) fn enqueue(&self, line: String, response: bool) -> Result<(), PushError> {
        self.outbox.push(line, response)
    }

    /// Drop a client that stopped draining its queue.
    pub(crate) fn overflowed(&self) {
        self.close(Some(RpcError::new(
            ErrorCode::SlowConsumer,
            "client did not read queued messages fast enough; connection closed",
        )));
    }

    /// Stop accepting work, end every subscription, and let the writer drain.
    pub(crate) fn close(&self, reason: Option<RpcError>) {
        let final_line = reason.map(|reason| {
            Message::notification(
                CLOSING_NOTIFICATION,
                json!({ "code": reason.code, "message": reason.message }),
            )
            .encode_line()
        });
        self.outbox.begin_close(final_line);
        let ended: Vec<_> = self
            .subscriptions()
            .drain()
            .map(|(_, state)| state)
            .collect();
        for state in ended {
            state.close();
        }
    }

    pub(crate) fn shutdown_socket(&self) {
        let _ = self.socket.shutdown(Shutdown::Both);
    }

    fn open_subscription(&self) -> Result<Arc<SubscriptionState>, RpcError> {
        let mut subscriptions = self.subscriptions();
        if !self.is_open() {
            return Err(RpcError::new(
                ErrorCode::ShuttingDown,
                "connection is closing",
            ));
        }
        if subscriptions.len() >= self.max_subscriptions {
            return Err(RpcError::new(
                ErrorCode::ServerBusy,
                format!(
                    "subscription limit of {} per connection reached",
                    self.max_subscriptions
                ),
            ));
        }
        let number = self.next_subscription.fetch_add(1, Ordering::Relaxed) + 1;
        let state = Arc::new(SubscriptionState::new(format!("sub-{number}")));
        subscriptions.insert(state.id().to_owned(), Arc::clone(&state));
        Ok(state)
    }

    pub(crate) fn remove_subscription(&self, id: &str) {
        self.subscriptions().remove(id);
    }

    fn unsubscribe(&self, id: &str) -> bool {
        let state = self.subscriptions().remove(id);
        state.map(|state| state.close()).is_some()
    }
}

/// Start the reader and writer threads for an accepted socket.
pub(crate) fn spawn(stream: TcpStream, server: &Arc<ServerShared>) -> io::Result<()> {
    let id = server.next_connection_id();
    let reader_stream = stream.try_clone()?;
    let writer_stream = stream.try_clone()?;
    writer_stream.set_write_timeout(Some(server.config.write_timeout))?;
    let connection = Arc::new(ConnectionShared::new(id, stream, server));
    server.insert(Arc::clone(&connection));
    let abort = |connection: &ConnectionShared| {
        connection.close(None);
        connection.shutdown_socket();
        server.remove(id);
    };

    let writer_connection = Arc::clone(&connection);
    let writer = match thread::Builder::new()
        .name(format!("aidexd-conn-{id}-write"))
        .spawn(move || write_loop(writer_stream, &writer_connection))
    {
        Ok(writer) => writer,
        Err(error) => {
            abort(&connection);
            return Err(error);
        }
    };
    let reader_connection = Arc::clone(&connection);
    let reader_server = Arc::clone(server);
    let spawned = thread::Builder::new()
        .name(format!("aidexd-conn-{id}-read"))
        .spawn(move || {
            let mut reader = LineReader::new(reader_stream, MAX_HANDSHAKE_BYTES);
            read_loop(&mut reader, &reader_connection, &reader_server);
            reader_connection
                .reader_finished
                .store(true, Ordering::SeqCst);
            reader_connection.close(None);
            let _ = writer.join();
            linger(reader.stream());
            reader_connection.shutdown_socket();
            reader_server.remove(id);
        });
    if let Err(error) = spawned {
        abort(&connection);
        return Err(error);
    }
    Ok(())
}

fn write_loop(mut stream: TcpStream, connection: &ConnectionShared) {
    while let Pop::Line(line) = connection.outbox.pop() {
        if stream.write_all(line.as_bytes()).is_err() {
            break;
        }
    }
    connection.close(None);
    connection.outbox.finish();
    if connection.reader_finished.load(Ordering::SeqCst) {
        // The reader is lingering: send FIN after the queued data.
        let _ = stream.shutdown(Shutdown::Write);
    } else {
        // Wake a reader blocked on an idle connection.
        connection.shutdown_socket();
    }
}

fn linger(mut stream: &TcpStream) {
    let deadline = Instant::now() + LINGER;
    let mut discarded = 0;
    let mut buffer = [0u8; 8192];
    while discarded < LINGER_BYTES {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || stream.set_read_timeout(Some(remaining)).is_err() {
            return;
        }
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => discarded += read,
        }
    }
}

/// Answer a protocol violation and end the connection. The reader stops
/// afterwards, so mark it finished first: the writer then half-closes and the
/// reader lingers instead of resetting the socket under the error line.
fn reject(connection: &ConnectionShared, id: Id, error: RpcError) {
    connection.reader_finished.store(true, Ordering::SeqCst);
    let _ = connection.enqueue(Message::response(id, Err(error)).encode_line(), true);
    connection.close(None);
}

fn read_loop(reader: &mut LineReader, connection: &Arc<ConnectionShared>, server: &ServerShared) {
    let config = &server.config;
    if reader
        .set_deadline(Some(Instant::now() + config.handshake_timeout))
        .is_err()
    {
        return;
    }
    let Some(client) = authenticate(reader, connection, server) else {
        return;
    };
    reader.set_limit(config.max_message_bytes);
    if reader.set_deadline(None).is_err() {
        return;
    }
    let context = CallContext::new(
        connection.id(),
        Arc::new(client),
        Arc::clone(connection),
        server.shutdown.clone(),
    );
    loop {
        let line = match reader.read_frame() {
            Ok(Frame::Line(line)) => line,
            Ok(Frame::TooLarge) => {
                reject(
                    connection,
                    Id::Null,
                    RpcError::new(
                        ErrorCode::MessageTooLarge,
                        format!(
                            "message exceeds the {}-byte line limit",
                            config.max_message_bytes
                        ),
                    ),
                );
                return;
            }
            Ok(Frame::Eof) | Err(_) => return,
        };
        if !connection.outbox.begin_request() {
            return;
        }
        if let Some(reply) = dispatch(&line, &context, connection, server)
            && connection.enqueue(reply, true) == Err(PushError::Full)
        {
            connection.overflowed();
        }
        connection.outbox.end_request();
    }
}

fn authenticate(
    reader: &mut LineReader,
    connection: &ConnectionShared,
    server: &ServerShared,
) -> Option<ClientInfo> {
    let line = match reader.read_frame() {
        Ok(Frame::Line(line)) => line,
        Ok(Frame::TooLarge) => {
            reject(
                connection,
                Id::Null,
                RpcError::new(
                    ErrorCode::MessageTooLarge,
                    format!("handshake exceeds {MAX_HANDSHAKE_BYTES} bytes"),
                ),
            );
            return None;
        }
        Ok(Frame::Eof) => return None,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
            ) =>
        {
            reject(
                connection,
                Id::Null,
                RpcError::new(
                    ErrorCode::HandshakeRequired,
                    "no `aidex.handshake` request received in time",
                ),
            );
            return None;
        }
        Err(_) => return None,
    };
    match handshake::authenticate(&line, &server.token) {
        Ok((id, client)) => {
            let result = HandshakeResult {
                contract_version: CONTRACT_VERSION,
                server: ServerInfo::current(),
                connection_id: connection.id(),
            };
            let line = Message::response(id, to_result(&result)).encode_line();
            connection.enqueue(line, true).ok().map(|()| client)
        }
        Err(Malformed { id, error }) => {
            reject(connection, id, error);
            None
        }
    }
}

fn invoke(handler: impl FnOnce() -> Result<Value, RpcError>) -> Result<Value, RpcError> {
    catch_unwind(AssertUnwindSafe(handler))
        .unwrap_or_else(|_| Err(RpcError::internal("method handler panicked")))
}

/// Encode a response, replacing results that exceed the line limit.
fn respond(id: Id, outcome: Result<Value, RpcError>, limit: usize) -> String {
    let line = Message::response(id.clone(), outcome).encode_line();
    if line.len() - 1 <= limit {
        return line;
    }
    Message::response(
        id,
        Err(RpcError::new(
            ErrorCode::MessageTooLarge,
            format!("result exceeds the {limit}-byte line limit"),
        )),
    )
    .encode_line()
}

fn dispatch(
    line: &[u8],
    context: &CallContext,
    connection: &Arc<ConnectionShared>,
    server: &ServerShared,
) -> Option<String> {
    let limit = server.config.max_message_bytes;
    let message = match Message::parse(line) {
        Ok(message) => message,
        Err(Malformed { id, error }) => return Some(respond(id, Err(error), limit)),
    };
    match message {
        Message::Request { id, method, params } => {
            let params = params.unwrap_or(Value::Null);
            let outcome = if method == HANDSHAKE_METHOD {
                Err(RpcError::invalid_request(
                    "connection is already authenticated",
                ))
            } else if method == UNSUBSCRIBE_METHOD {
                unsubscribe(connection, params)
            } else {
                match server.router.route(&method) {
                    Some(Route::Call(handler)) => invoke(|| handler(context, params)),
                    Some(Route::Stream(handler)) => {
                        return subscribe(id, handler.as_ref(), context, connection, params, limit);
                    }
                    None => Err(RpcError::method_not_found(&method)),
                }
            };
            Some(respond(id, outcome, limit))
        }
        Message::Notification { method, params } => {
            let params = params.unwrap_or(Value::Null);
            if method == UNSUBSCRIBE_METHOD {
                let _ = unsubscribe(connection, params);
            } else if let Some(Route::Call(handler)) = server.router.route(&method) {
                let _ = invoke(|| handler(context, params));
            }
            None
        }
        Message::Response { .. } => Some(respond(
            Id::Null,
            Err(RpcError::invalid_request("clients must not send responses")),
            limit,
        )),
    }
}

fn subscribe(
    id: Id,
    handler: &StreamHandler,
    context: &CallContext,
    connection: &Arc<ConnectionShared>,
    params: Value,
    limit: usize,
) -> Option<String> {
    let state = match connection.open_subscription() {
        Ok(state) => state,
        Err(error) => return Some(respond(id, Err(error), limit)),
    };
    let sink = NotificationSink::new(Arc::clone(&state), Arc::clone(connection));
    let outcome = invoke(|| handler(context, params, sink)).and_then(|data| {
        let line = Message::response(
            id.clone(),
            Ok(json!({ "subscription": state.id(), "data": data })),
        )
        .encode_line();
        if line.len() - 1 <= limit {
            Ok(line)
        } else {
            Err(RpcError::new(
                ErrorCode::MessageTooLarge,
                format!("subscribe result exceeds the {limit}-byte line limit"),
            ))
        }
    });
    match outcome {
        Ok(line) => {
            state.activate(connection, line);
            None
        }
        Err(error) => {
            state.close();
            connection.remove_subscription(state.id());
            Some(respond(id, Err(error), limit))
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnsubscribeParams {
    subscription: String,
}

fn unsubscribe(connection: &ConnectionShared, params: Value) -> Result<Value, RpcError> {
    let params: UnsubscribeParams = parse_params(params)?;
    if connection.unsubscribe(&params.subscription) {
        Ok(json!({ "unsubscribed": true }))
    } else {
        Err(RpcError::new(
            ErrorCode::NotFound,
            "no active subscription with that id on this connection",
        ))
    }
}
