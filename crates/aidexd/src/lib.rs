//! `aidexd`: the local AiDEX daemon and its blocking IPC client.
//!
//! IPC contract version 2 is JSON-RPC 2.0 over loopback TCP: newline-delimited
//! UTF-8 messages of at most 1 MiB on a persistent connection whose first
//! request must be an authenticated `aidex.handshake`. The endpoint file at
//! `<drive>/.aidex/runtime/daemon.json` names the port and carries a 256-bit
//! OS-random capability (DPAPI-protected on Windows, 0600 on Unix).
//! See `protocol/commands/README.md` for the wire contract and error codes.

pub mod client;
pub mod endpoint;
pub mod errors;
mod framing;
pub mod handshake;
pub mod lifecycle;
pub mod methods;
pub mod router;
pub mod rpc;
pub mod services;
pub mod stream;
mod transport;

pub use client::{Client, ClientError, Notification, Subscription};
pub use endpoint::{Capability, Endpoint, EndpointError};
pub use errors::{ErrorCode, RpcError};
pub use lifecycle::{
    Daemon, DaemonError, DaemonHandle, DaemonOptions, Recovery, ShutdownSignal, StopReason,
};
pub use router::{CallContext, ClientInfo, Router, RouterError};
pub use rpc::{CONTRACT_VERSION, MAX_MESSAGE_BYTES};
pub use services::{Services, ServicesError};
pub use stream::{NotificationSink, SinkError};
pub use transport::TransportConfig;
