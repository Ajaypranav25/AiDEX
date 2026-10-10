//! `aidex events`: print durable workspace events as NDJSON, optionally
//! following and reconnecting from the last flushed cursor.
//!
//! Uses `events.replay` over IPC v2 (ported from the contract-v1 `events`
//! request of PR #40). Output is always one JSON event per line.

use crate::{
    commands::daemon::connect_root,
    exit::{CliError, Exit},
    output::Output,
};
use aidex_drive::resolve_root;
use aidex_events::{Event, MAX_BATCH};
use aidexd::{
    Client,
    methods::events::{REPLAY, ReplayBatch},
};
use clap::Args;
use serde_json::json;
use std::{
    io::{self, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const RECONNECT_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Args)]
pub struct EventsArgs {
    /// Workspace whose events to read.
    #[arg(long)]
    pub workspace: String,
    /// Last successfully emitted event sequence.
    #[arg(long, default_value_t = 0)]
    pub after: u64,
    /// Drive root; defaults to AIDEX_DRIVE_ROOT from the portable launcher.
    #[arg(long, value_name = "DRIVE_PATH")]
    pub drive: Option<PathBuf>,
    /// Continue polling and reconnect after daemon outages.
    #[arg(long)]
    pub follow: bool,
}

pub fn run(args: EventsArgs, _output: Output) -> Result<Exit, CliError> {
    let root = resolve_root(args.drive.as_deref())?;
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let mut cursor = args.after;
    let mut client: Option<Client> = None;
    loop {
        let connection = match client.as_mut() {
            Some(connection) => connection,
            None => match connect_root(&root) {
                Ok(connection) => client.insert(connection),
                Err(error) if args.follow && error.exit == Exit::DaemonUnavailable => {
                    eprintln!("aidex events: reconnecting after error: {}", error.message);
                    thread::sleep(RECONNECT_DELAY);
                    continue;
                }
                Err(error) => return Err(error),
            },
        };
        let params = json!({ "workspace_id": args.workspace, "after": cursor, "limit": MAX_BATCH });
        match connection.call_as::<ReplayBatch>(REPLAY, params) {
            Ok(batch) => {
                emit_events(&batch.events, &mut stdout, &mut cursor).map_err(|error| {
                    CliError::new(Exit::Failure, format!("cannot emit event: {error}"))
                })?;
                if !args.follow {
                    return Ok(Exit::Success);
                }
                if batch.events.is_empty() {
                    thread::sleep(POLL_INTERVAL);
                }
            }
            Err(error) => {
                let error = CliError::from(error);
                if !(args.follow && error.exit == Exit::DaemonUnavailable) {
                    return Err(error);
                }
                eprintln!("aidex events: reconnecting after error: {}", error.message);
                client = None;
                thread::sleep(RECONNECT_DELAY);
            }
        }
    }
}

/// Write events in order, advancing `acknowledged` only after each flush.
fn emit_events<W: Write>(
    events: &[Event],
    output: &mut W,
    acknowledged: &mut u64,
) -> io::Result<()> {
    for event in events {
        serde_json::to_writer(&mut *output, event).map_err(io::Error::other)?;
        output.write_all(b"\n")?;
        output.flush()?;
        *acknowledged = event.sequence;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aidex_events::{EVENT_VERSION, EventKind};

    struct TestWriter {
        bytes: Vec<u8>,
        flushes: usize,
        fail_flush: Option<usize>,
    }

    impl Write for TestWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush == Some(self.flushes) {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "consumer disconnected",
                ))
            } else {
                Ok(())
            }
        }
    }

    fn event(sequence: u64) -> Event {
        Event {
            version: EVENT_VERSION,
            sequence,
            workspace_id: "workspace".into(),
            session_id: Some("session".into()),
            kind: EventKind::SessionStarted,
        }
    }

    #[test]
    fn cli_acknowledges_only_successfully_flushed_events() {
        let events = [event(4), event(5)];
        let mut writer = TestWriter {
            bytes: Vec::new(),
            flushes: 0,
            fail_flush: Some(2),
        };
        let mut cursor = 3;

        assert!(emit_events(&events, &mut writer, &mut cursor).is_err());
        assert_eq!(cursor, 4);
        let lines = std::str::from_utf8(&writer.bytes)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert_eq!(serde_json::from_str::<Event>(lines[0]).unwrap().sequence, 4);
    }

    #[test]
    fn cli_replay_batch_advances_cursor_in_event_order() {
        let events = [event(1), event(2), event(3)];
        let mut writer = TestWriter {
            bytes: Vec::new(),
            flushes: 0,
            fail_flush: None,
        };
        let mut cursor = 0;

        emit_events(&events, &mut writer, &mut cursor).unwrap();

        assert_eq!(cursor, 3);
        let emitted = std::str::from_utf8(&writer.bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Event>(line).unwrap().sequence)
            .collect::<Vec<_>>();
        assert_eq!(emitted, [1, 2, 3]);
    }
}
