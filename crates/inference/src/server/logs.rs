//! Bounded server logs in `logs/inference/`, with the launch key redacted and
//! the last lines kept in memory for error messages.

use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use aidex_sandbox::ManagedProcess;
use zeroize::Zeroizing;

use crate::{
    InferenceError, LocalModelId, Result,
    fsutil::{self, display},
};

/// Lines kept for error reports.
const TAIL_LINES: usize = 24;
/// Longest error detail built from the tail.
const MAX_DETAIL: usize = 2048;
/// How often captured output is moved into the log file.
const PUMP_INTERVAL: Duration = Duration::from_millis(200);

struct LogState {
    file: Option<File>,
    written: u64,
    truncated: bool,
    tail: VecDeque<String>,
    partial: String,
}

/// One server's log file.
pub(crate) struct ServerLog {
    path: PathBuf,
    limit: u64,
    secret: Zeroizing<String>,
    state: Mutex<LogState>,
}

impl ServerLog {
    /// Create `logs_dir/llama-server-<model>-<millis>.log`, keeping at most
    /// `keep` older logs of the same model.
    pub(crate) fn create(
        logs_dir: &Path,
        model: &LocalModelId,
        limit: u64,
        keep: usize,
        secret: &str,
    ) -> Result<Self> {
        prune(logs_dir, model, keep);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());
        let path = logs_dir.join(format!(
            "llama-server-{model}-{stamp}-{}.log",
            &fsutil::random_suffix()?[..6]
        ));
        let file = OpenOptions::new()
            .append(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                InferenceError::io(format!("cannot create {}", display(&path)), &error)
            })?;
        Ok(Self {
            path,
            limit,
            secret: Zeroizing::new(secret.to_owned()),
            state: Mutex::new(LogState {
                file: Some(file),
                written: 0,
                truncated: false,
                tail: VecDeque::new(),
                partial: String::new(),
            }),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    fn redact(&self, text: &str) -> String {
        if self.secret.is_empty() {
            text.to_owned()
        } else {
            text.replace(self.secret.as_str(), "[REDACTED]")
        }
    }

    /// Append raw output (bounded; the launch key is never written).
    pub(crate) fn append(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let text = self.redact(&String::from_utf8_lossy(bytes));
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.partial.push_str(&text);
        while let Some(at) = state.partial.find('\n') {
            let line: String = state.partial.drain(..=at).collect();
            let line = line.trim_end().to_owned();
            if !line.is_empty() {
                state.tail.push_back(line);
                if state.tail.len() > TAIL_LINES {
                    state.tail.pop_front();
                }
            }
        }
        if state.partial.len() > 64 * 1024 {
            state.partial.clear();
        }
        let room = self.limit.saturating_sub(state.written);
        let keep = (text.len() as u64).min(room) as usize;
        let mut keep = keep;
        while keep > 0 && !text.is_char_boundary(keep) {
            keep -= 1;
        }
        let truncate = keep < text.len() && !state.truncated;
        let written = keep as u64;
        if let Some(file) = state.file.as_mut() {
            if keep > 0 {
                let _ = file.write_all(&text.as_bytes()[..keep]);
            }
            if truncate {
                let _ = writeln!(
                    file,
                    "\n[aidex: log limit of {} bytes reached; further output dropped]",
                    self.limit
                );
            }
        }
        state.written = state.written.saturating_add(written);
        if truncate {
            state.truncated = true;
        }
    }

    /// A note from AiDEX itself (launch arguments, exit status).
    pub(crate) fn note(&self, line: &str) {
        if let Ok(mut state) = self.state.lock() {
            let text = self.redact(line);
            if let Some(file) = state.file.as_mut() {
                let _ = writeln!(file, "[aidex] {text}");
            }
        }
    }

    /// The last lines, joined for an error message.
    pub(crate) fn tail_detail(&self) -> String {
        let Ok(state) = self.state.lock() else {
            return String::new();
        };
        let mut detail = String::new();
        for line in state
            .tail
            .iter()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            if !detail.is_empty() {
                detail.push_str(" | ");
            }
            detail.push_str(line);
        }
        if detail.len() > MAX_DETAIL {
            let mut cut = detail.len() - MAX_DETAIL;
            while !detail.is_char_boundary(cut) {
                cut += 1;
            }
            detail = detail[cut..].to_owned();
        }
        detail
    }

    pub(crate) fn tail_contains(&self, needle: &str) -> bool {
        self.state
            .lock()
            .map(|state| {
                state
                    .tail
                    .iter()
                    .any(|line| line.to_ascii_lowercase().contains(needle))
            })
            .unwrap_or(false)
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(file) = state.file.take() {
                let _ = file.sync_all();
            }
        }
    }
}

/// Drop the oldest logs of `model` beyond `keep`.
fn prune(logs_dir: &Path, model: &LocalModelId, keep: usize) {
    let prefix = format!("llama-server-{model}-");
    let Ok(read) = fs::read_dir(logs_dir) else {
        return;
    };
    let mut logs: Vec<PathBuf> = read
        .flatten()
        .map(|item| item.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".log"))
        })
        .collect();
    logs.sort();
    while logs.len() >= keep.max(1) {
        let oldest = logs.remove(0);
        let _ = fs::remove_file(oldest);
    }
}

/// Moves captured process output into the log until stopped.
pub(crate) struct LogPump {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LogPump {
    pub(crate) fn start(process: Arc<ManagedProcess>, log: Arc<ServerLog>) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("aidex-llama-log".into())
            .spawn(move || {
                loop {
                    drain(&process, &log);
                    if flag.load(Ordering::SeqCst) {
                        return;
                    }
                    thread::sleep(PUMP_INTERVAL);
                }
            })
            .map_err(|error| InferenceError::io("cannot start the log thread", &error))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }

    pub(crate) fn finish(&mut self, log: &ServerLog) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        log.close();
    }
}

impl Drop for LogPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn drain(process: &ManagedProcess, log: &ServerLog) {
    let output = process.take_output();
    log.append(&output.stdout.bytes);
    log.append(&output.stderr.bytes);
}
