//! One HTTP exchange on a worker thread, consumed with cancellation.
//!
//! `ureq` is blocking, so the exchange runs on its own thread and hands frames
//! (status, then SSE lines or the bounded body) over a bounded channel. The
//! caller polls the channel and its [`CancelToken`]; on cancellation or deadline
//! it stops waiting immediately and drops the channel, and the worker exits at
//! its next read, closing the connection (servers such as llama-server stop
//! generating when the client disconnects). A worker blocked on a silent server
//! is bounded by the agent's body timeout.

use std::{
    io::{self, BufRead, BufReader, Read},
    sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel},
    thread,
    time::Duration,
};

use aidex_core::{CancelReason, CancelToken};
use zeroize::Zeroizing;

use crate::ProviderError;

/// Longest wait between cancellation checks.
const POLL: Duration = Duration::from_millis(20);
/// Frames buffered between the worker and the caller.
const CHANNEL_DEPTH: usize = 64;
/// Error bodies are read only this far (they are parsed for a code, never shown).
const MAX_ERROR_BODY: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub max_body: u64,
    pub max_line: usize,
}

pub(crate) enum Method {
    Get,
    Post(Vec<u8>),
}

pub(crate) struct HttpRequest {
    pub url: String,
    pub method: Method,
    /// Full `Authorization` header value; moved into the worker and zeroized
    /// once the request is sent.
    pub authorization: Option<Zeroizing<String>>,
    pub headers: Vec<(String, Zeroizing<String>)>,
    pub stream: bool,
}

pub(crate) enum Frame {
    Head {
        status: u16,
        retry_after: Option<Duration>,
        event_stream: bool,
    },
    /// One line of a successful `text/event-stream` body (without the newline).
    Line(String),
    Event {
        event_type: String,
        payload: String,
    },
    /// A complete body (non-streaming success, or an error body).
    Body(String),
    End,
}

type Message = Result<Frame, ProviderError>;

pub(crate) struct Exchange {
    frames: Receiver<Message>,
}

impl Exchange {
    pub fn start(
        agent: &ureq::Agent,
        request: HttpRequest,
        limits: Limits,
        cancel: &CancelToken,
    ) -> Result<Self, ProviderError> {
        check_cancel(cancel)?;
        let (sender, frames) = sync_channel(CHANNEL_DEPTH);
        let agent = agent.clone();
        let cancel = cancel.clone();
        thread::Builder::new()
            .name("aidex-provider-http".into())
            .spawn(move || run(&agent, request, limits, &cancel, &sender))
            .map_err(|_| ProviderError::unavailable("cannot start an HTTP worker thread"))?;
        Ok(Self { frames })
    }

    /// The next frame, or `Cancelled`/`Timeout` as soon as `cancel` stops.
    pub fn next(&self, cancel: &CancelToken) -> Result<Frame, ProviderError> {
        loop {
            match cancel.state() {
                Some(CancelReason::Cancelled) => return Err(ProviderError::Cancelled),
                Some(CancelReason::DeadlineExceeded) => return Err(ProviderError::Timeout),
                None => {}
            }
            let wait = cancel.remaining().map_or(POLL, |left| left.min(POLL));
            match self.frames.recv_timeout(wait) {
                Ok(message) => return message,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(ProviderError::Protocol(
                        "the HTTP exchange ended unexpectedly".into(),
                    ));
                }
            }
        }
    }
}

pub(crate) fn check_cancel(cancel: &CancelToken) -> Result<(), ProviderError> {
    match cancel.state() {
        Some(CancelReason::Cancelled) => Err(ProviderError::Cancelled),
        Some(CancelReason::DeadlineExceeded) => Err(ProviderError::Timeout),
        None => Ok(()),
    }
}

fn run(
    agent: &ureq::Agent,
    request: HttpRequest,
    limits: Limits,
    cancel: &CancelToken,
    sender: &SyncSender<Message>,
) {
    if let Err(error) = check_cancel(cancel) {
        let _ = sender.send(Err(error));
        return;
    }
    let HttpRequest {
        url,
        method,
        authorization,
        headers,
        stream,
    } = request;
    let accept = if stream {
        "text/event-stream, application/vnd.amazon.eventstream, application/json"
    } else {
        "application/json"
    };
    let response = match method {
        Method::Get => {
            let mut builder = agent.get(&url).header("Accept", accept);
            for (name, value) in &headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            if let Some(value) = &authorization {
                builder = builder.header("Authorization", value.as_str());
            }
            match check_cancel(cancel) {
                Ok(()) => builder.call(),
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            }
        }
        Method::Post(body) => {
            let mut builder = agent
                .post(&url)
                .header("Accept", accept)
                .content_type("application/json");
            for (name, value) in &headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            if let Some(value) = &authorization {
                builder = builder.header("Authorization", value.as_str());
            }
            match check_cancel(cancel) {
                Ok(()) => builder.send(&body[..]),
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            }
        }
    };
    drop(authorization);
    drop(headers);
    let mut response = match response {
        Ok(response) => response,
        Err(error) => {
            let _ = sender.send(Err(map_ureq_error(error)));
            return;
        }
    };
    let status = response.status().as_u16();
    let headers = response.headers();
    let retry_after = headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    let aws_stream = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/vnd.amazon.eventstream"));
    let event_stream = aws_stream
        || headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("text/event-stream")
            });
    if sender
        .send(Ok(Frame::Head {
            status,
            retry_after,
            event_stream,
        }))
        .is_err()
    {
        return;
    }
    let success = (200..300).contains(&status);
    let mut reader = BufReader::new(response.body_mut().as_reader());
    if success && aws_stream {
        loop {
            let message = match super::eventstream::read_frame(&mut reader, limits.max_line) {
                Ok(Some((event_type, payload))) => Ok(Frame::Event {
                    event_type,
                    payload,
                }),
                Ok(None) => Ok(Frame::End),
                Err(error) => Err(map_body_error(&error)),
            };
            let last = !matches!(message, Ok(Frame::Event { .. }));
            if sender.send(message).is_err() || last {
                return;
            }
        }
    }
    if success && event_stream {
        loop {
            let message = match read_line_bounded(&mut reader, limits.max_line) {
                Ok(Some(line)) => Ok(Frame::Line(line)),
                Ok(None) => Ok(Frame::End),
                Err(error) => Err(map_body_error(&error)),
            };
            let last = !matches!(message, Ok(Frame::Line(_)));
            if sender.send(message).is_err() || last {
                return;
            }
        }
    }
    let limit = if success {
        limits.max_body
    } else {
        MAX_ERROR_BODY
    };
    let mut bytes = Vec::new();
    let message = match reader.take(limit + 1).read_to_end(&mut bytes) {
        Ok(_) if success && bytes.len() as u64 > limit => Err(ProviderError::Protocol(
            "response body exceeds the size limit".into(),
        )),
        Ok(_) => {
            bytes.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
            Ok(Frame::Body(String::from_utf8_lossy(&bytes).into_owned()))
        }
        // The status already describes a failed exchange; an unreadable error
        // body only loses the optional machine code.
        Err(_) if !success => Ok(Frame::Body(String::new())),
        Err(error) => Err(map_body_error(&error)),
    };
    let ok = message.is_ok();
    if sender.send(message).is_ok() && ok {
        let _ = sender.send(Ok(Frame::End));
    }
}

/// Read one `\n`-terminated line (dropping a trailing `\r`), refusing lines
/// longer than `max` bytes; `Ok(None)` at end of stream.
pub(crate) fn read_line_bounded(
    reader: &mut impl BufRead,
    max: usize,
) -> io::Result<Option<String>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            break;
        }
        if let Some(position) = available.iter().position(|&byte| byte == b'\n') {
            line.extend_from_slice(&available[..position]);
            reader.consume(position + 1);
            break;
        }
        let length = available.len();
        line.extend_from_slice(available);
        reader.consume(length);
        if line.len() > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "stream line exceeds the size limit",
            ));
        }
    }
    if line.len() > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "stream line exceeds the size limit",
        ));
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    String::from_utf8(line)
        .map(Some)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "stream is not UTF-8"))
}

fn map_body_error(error: &io::Error) -> ProviderError {
    match error.kind() {
        io::ErrorKind::InvalidData => ProviderError::Protocol(error.to_string()),
        io::ErrorKind::TimedOut => ProviderError::Timeout,
        _ => ProviderError::Protocol("the connection was lost while reading the response".into()),
    }
}

fn map_ureq_error(error: ureq::Error) -> ProviderError {
    match error {
        ureq::Error::Timeout(_) => ProviderError::Timeout,
        ureq::Error::HostNotFound => ProviderError::unavailable("host not found"),
        ureq::Error::ConnectionFailed => ProviderError::unavailable("connection failed"),
        ureq::Error::Io(error) => match error.kind() {
            io::ErrorKind::TimedOut => ProviderError::Timeout,
            io::ErrorKind::ConnectionRefused => ProviderError::unavailable("connection refused"),
            kind => ProviderError::unavailable(format!("connection error ({kind})")),
        },
        ureq::Error::Protocol(_) => {
            ProviderError::Protocol("the server sent a malformed HTTP response".into())
        }
        other => ProviderError::unavailable(format!("request failed: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_lines_split_on_newlines_and_reject_oversized_lines() {
        let mut reader = io::Cursor::new(b"data: a\r\n\ndata: b".to_vec());
        assert_eq!(
            read_line_bounded(&mut reader, 16).unwrap().unwrap(),
            "data: a"
        );
        assert_eq!(read_line_bounded(&mut reader, 16).unwrap().unwrap(), "");
        assert_eq!(
            read_line_bounded(&mut reader, 16).unwrap().unwrap(),
            "data: b"
        );
        assert!(read_line_bounded(&mut reader, 16).unwrap().is_none());
        let mut long = io::Cursor::new(vec![b'x'; 100]);
        assert!(read_line_bounded(&mut long, 16).is_err());
    }
}
