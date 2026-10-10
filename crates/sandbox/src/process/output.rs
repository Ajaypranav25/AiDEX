//! Bounded capture of a sandboxed tree's stdout and stderr.

use serde::Serialize;

/// Captured bytes of one stream.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct StreamCapture {
    /// Bytes retained (and not yet taken).
    #[serde(skip)]
    pub bytes: Vec<u8>,
    /// Whether any output was dropped because the buffer was full.
    pub truncated: bool,
    /// Every byte the stream produced so far, retained or not.
    pub total_bytes: u64,
    /// Bytes dropped so far.
    pub dropped_bytes: u64,
}

impl StreamCapture {
    /// Lossy UTF-8 view of the retained bytes.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// Both captured streams.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct CapturedOutput {
    pub stdout: StreamCapture,
    pub stderr: StreamCapture,
}

/// Which stream a reader drains.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stream {
    Stdout,
    Stderr,
}

/// A stream buffer that never holds more than `max_bytes`. Output arriving while it is
/// full is drained from the pipe (so the child never blocks) and counted as dropped.
#[derive(Debug)]
pub(crate) struct StreamBuffer {
    capture: StreamCapture,
    max_bytes: usize,
}

impl StreamBuffer {
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self {
            capture: StreamCapture::default(),
            max_bytes,
        }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) {
        let length = chunk.len() as u64;
        self.capture.total_bytes = self.capture.total_bytes.saturating_add(length);
        let room = self.max_bytes.saturating_sub(self.capture.bytes.len());
        let kept = room.min(chunk.len());
        self.capture.bytes.extend_from_slice(&chunk[..kept]);
        if kept < chunk.len() {
            self.capture.truncated = true;
            self.capture.dropped_bytes = self
                .capture
                .dropped_bytes
                .saturating_add((chunk.len() - kept) as u64);
        }
    }

    pub(crate) fn snapshot(&self) -> StreamCapture {
        self.capture.clone()
    }

    /// Return the retained bytes and free the buffer; counters keep accumulating.
    pub(crate) fn take(&mut self) -> StreamCapture {
        let bytes = std::mem::take(&mut self.capture.bytes);
        StreamCapture {
            bytes,
            ..self.capture.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_is_bounded_and_flags_truncation() {
        let mut buffer = StreamBuffer::new(8);
        buffer.push(b"hello");
        assert!(!buffer.snapshot().truncated);
        buffer.push(b" world");
        let capture = buffer.snapshot();
        assert_eq!(capture.bytes, b"hello wo");
        assert!(capture.truncated);
        assert_eq!(capture.total_bytes, 11);
        assert_eq!(capture.dropped_bytes, 3);
    }

    #[test]
    fn take_frees_room_for_streaming_consumers() {
        let mut buffer = StreamBuffer::new(4);
        buffer.push(b"abcd");
        assert_eq!(buffer.take().bytes, b"abcd");
        buffer.push(b"efgh");
        let capture = buffer.take();
        assert_eq!(capture.bytes, b"efgh");
        assert!(!capture.truncated);
        assert_eq!(capture.total_bytes, 8);
        assert!(buffer.snapshot().bytes.is_empty());
    }

    #[test]
    fn zero_capacity_drops_everything() {
        let mut buffer = StreamBuffer::new(0);
        buffer.push(b"data");
        let capture = buffer.snapshot();
        assert!(capture.bytes.is_empty());
        assert!(capture.truncated);
        assert_eq!(capture.dropped_bytes, 4);
    }
}
