//! Bounded newline framing over a TCP stream.
//!
//! A line longer than the limit is reported as [`Frame::TooLarge`] without
//! buffering more than the limit plus one read buffer. Blank lines are skipped
//! and a trailing `\r` is stripped so CRLF senders interoperate.

use std::{
    io::{self, BufRead, BufReader},
    net::TcpStream,
    time::Instant,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    Line(Vec<u8>),
    TooLarge,
    Eof,
}

pub(crate) struct LineReader {
    inner: BufReader<TcpStream>,
    partial: Vec<u8>,
    limit: usize,
    deadline: Option<Instant>,
}

impl LineReader {
    pub(crate) fn new(stream: TcpStream, limit: usize) -> Self {
        Self {
            inner: BufReader::new(stream),
            partial: Vec::new(),
            limit,
            deadline: None,
        }
    }

    pub(crate) fn stream(&self) -> &TcpStream {
        self.inner.get_ref()
    }

    pub(crate) fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    /// Fail reads with `TimedOut` once `deadline` passes; `None` blocks.
    pub(crate) fn set_deadline(&mut self, deadline: Option<Instant>) -> io::Result<()> {
        self.deadline = deadline;
        if deadline.is_none() {
            self.inner.get_ref().set_read_timeout(None)?;
        }
        Ok(())
    }

    fn arm_timeout(&self) -> io::Result<()> {
        if let Some(deadline) = self.deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.inner.get_ref().set_read_timeout(Some(remaining))?;
        }
        Ok(())
    }

    pub(crate) fn read_frame(&mut self) -> io::Result<Frame> {
        loop {
            if self.inner.buffer().is_empty() {
                self.arm_timeout()?;
            }
            let available = match self.inner.fill_buf() {
                Ok(available) => available,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if available.is_empty() {
                if self.partial.is_empty() {
                    return Ok(Frame::Eof);
                }
                let line = std::mem::take(&mut self.partial);
                return Ok(finish(line));
            }
            match available.iter().position(|byte| *byte == b'\n') {
                Some(newline) => {
                    let oversized = self.partial.len() + newline > self.limit + 1;
                    if !oversized {
                        self.partial.extend_from_slice(&available[..newline]);
                    }
                    self.inner.consume(newline + 1);
                    let line = std::mem::take(&mut self.partial);
                    if oversized {
                        return Ok(Frame::TooLarge);
                    }
                    match finish(line) {
                        Frame::Line(line) if line.is_empty() => continue,
                        Frame::Line(line) if line.len() > self.limit => {
                            return Ok(Frame::TooLarge);
                        }
                        frame => return Ok(frame),
                    }
                }
                None => {
                    let length = available.len();
                    if self.partial.len() + length > self.limit + 1 {
                        self.inner.consume(length);
                        self.partial.clear();
                        return Ok(Frame::TooLarge);
                    }
                    self.partial.extend_from_slice(available);
                    self.inner.consume(length);
                }
            }
        }
    }
}

fn finish(mut line: Vec<u8>) -> Frame {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Frame::Line(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        net::{Shutdown, TcpListener},
        time::Duration,
    };

    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    #[test]
    fn frames_lines_skips_blanks_and_bounds_length() {
        let (mut client, server) = pair();
        let mut reader = LineReader::new(server, 8);
        client
            .write_all(b"one\r\n\n12345678\n123456789\nlast")
            .unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        assert_eq!(reader.read_frame().unwrap(), Frame::Line(b"one".to_vec()));
        assert_eq!(
            reader.read_frame().unwrap(),
            Frame::Line(b"12345678".to_vec())
        );
        assert_eq!(reader.read_frame().unwrap(), Frame::TooLarge);
        assert_eq!(reader.read_frame().unwrap(), Frame::Line(b"last".to_vec()));
        assert_eq!(reader.read_frame().unwrap(), Frame::Eof);
    }

    #[test]
    fn oversized_line_without_newline_is_rejected_without_unbounded_buffering() {
        let (mut client, server) = pair();
        let mut reader = LineReader::new(server, 1024);
        let writer = std::thread::spawn(move || {
            let chunk = vec![b'x'; 64 * 1024];
            for _ in 0..4 {
                if client.write_all(&chunk).is_err() {
                    break;
                }
            }
        });
        assert_eq!(reader.read_frame().unwrap(), Frame::TooLarge);
        assert!(reader.partial.capacity() < 128 * 1024);
        drop(reader);
        writer.join().unwrap();
    }

    #[test]
    fn deadline_bounds_a_silent_peer() {
        let (_client, server) = pair();
        let mut reader = LineReader::new(server, 64);
        reader
            .set_deadline(Some(Instant::now() + Duration::from_millis(100)))
            .unwrap();
        let error = reader.read_frame().unwrap_err();
        assert!(matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ));
    }
}
