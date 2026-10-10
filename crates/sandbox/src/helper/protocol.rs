//! Wire protocol between the parent and `aidex-sandbox-exec`.
//!
//! The parent writes `PLAN_MAGIC`, a little-endian `u32` length, and a JSON
//! [`HelperPlan`] to the helper's stdin; any remaining stdin bytes belong to the target.
//! Environment values never travel through this channel: the parent starts the helper
//! with the final child environment, which the target inherits.
//!
//! After applying every restriction the helper writes [`READY_FRAME`] to stdout. On
//! failure it writes one [`FAILURE_PREFIX`] line to stderr and exits with
//! [`HELPER_FAILURE_EXIT`]; the target never runs unconfined.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

/// Protocol version carried in plans and probe reports.
pub const PROTOCOL_VERSION: u32 = 1;
/// Leading bytes of a framed plan.
pub const PLAN_MAGIC: &[u8; 8] = b"AIDEXSB1";
/// Upper bound for a framed plan.
pub const MAX_PLAN_BYTES: u32 = 1024 * 1024;
/// Written to stdout once all restrictions are in place, just before the target starts.
pub const READY_FRAME: &[u8] = b"\x01AIDEX-SANDBOX-READY\n";
/// Prefix of the single stderr line describing a setup failure.
pub const FAILURE_PREFIX: &[u8] = b"\x01AIDEX-SANDBOX-FAILED ";
/// Exit code of a helper that could not apply the plan.
pub const HELPER_FAILURE_EXIT: i32 = 125;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// Read-only (read, list, execute) system paths a Landlock-confined process needs to
/// run ordinary binaries. Missing paths are skipped.
pub const LINUX_SYSTEM_READ_PATHS: &[&str] = &[
    "/bin",
    "/sbin",
    "/usr",
    "/lib",
    "/lib32",
    "/lib64",
    "/libx32",
    "/etc",
    "/opt",
    "/nix/store",
    "/proc",
    "/sys",
];
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// Device files a Landlock-confined process may read and write. `/dev/tty` and block
/// devices are deliberately absent.
pub const LINUX_DEVICE_FILES: &[&str] = &[
    "/dev/null",
    "/dev/zero",
    "/dev/full",
    "/dev/random",
    "/dev/urandom",
];

/// Kernel resource limits applied by the helper to itself before `exec`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RlimitPlan {
    pub max_tasks: Option<u64>,
    pub address_space_bytes: Option<u64>,
    pub cpu_seconds: Option<u64>,
}

/// Landlock rules (the helper adds the system paths).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LandlockPlan {
    pub read_only: Vec<String>,
    pub read_write: Vec<String>,
}

/// A `sandbox-exec` invocation.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SeatbeltPlan {
    pub sandbox_exec: String,
    pub profile: String,
    pub params: Vec<(String, String)>,
}

/// Everything the helper applies before starting the target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HelperPlan {
    pub version: u32,
    pub program: String,
    pub args: Vec<String>,
    /// The parent process id; the helper refuses to run if it was re-parented.
    pub parent_pid: u32,
    pub rlimits: RlimitPlan,
    pub user_namespace: bool,
    pub network_namespace: bool,
    pub landlock: Option<LandlockPlan>,
    pub seatbelt: Option<SeatbeltPlan>,
}

/// Kernel facilities observed by `aidex-sandbox-exec --probe` (Linux).
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProbeReport {
    pub version: u32,
    /// Highest Landlock ABI for which a hard-requirement ruleset could be created.
    pub landlock_abi: Option<u32>,
    /// Whether a ruleset could actually be enforced on the probe process.
    pub landlock_enforced: bool,
    pub landlock_error: Option<String>,
    pub user_namespace: bool,
    pub user_namespace_error: Option<String>,
    pub network_namespace: bool,
    pub network_namespace_error: Option<String>,
}

/// Frame a plan for the helper's stdin.
pub fn encode_plan(plan: &HelperPlan) -> io::Result<Vec<u8>> {
    let json = serde_json::to_vec(plan).map_err(io::Error::other)?;
    let length = u32::try_from(json.len())
        .ok()
        .filter(|length| *length <= MAX_PLAN_BYTES)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "helper plan is too large"))?;
    let mut framed = Vec::with_capacity(PLAN_MAGIC.len() + 4 + json.len());
    framed.extend_from_slice(PLAN_MAGIC);
    framed.extend_from_slice(&length.to_le_bytes());
    framed.extend_from_slice(&json);
    Ok(framed)
}

/// Read exactly one framed plan. The reader must be unbuffered so that bytes after the
/// frame stay in the pipe for the target.
pub fn read_plan(reader: &mut impl Read) -> io::Result<HelperPlan> {
    let mut magic = [0_u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != PLAN_MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad plan magic"));
    }
    let mut length = [0_u8; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length);
    if length > MAX_PLAN_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "plan too large"));
    }
    let mut json = vec![0_u8; length as usize];
    reader.read_exact(&mut json)?;
    let plan: HelperPlan = serde_json::from_slice(&json)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if plan.version != PROTOCOL_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported plan version {}", plan.version),
        ));
    }
    Ok(plan)
}

/// Write a failure line: `FAILURE_PREFIX stage \t message \n`.
pub fn write_failure(writer: &mut impl Write, stage: &str, message: &str) -> io::Result<()> {
    let clean = |text: &str| -> String {
        text.chars()
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .collect()
    };
    let mut line = FAILURE_PREFIX.to_vec();
    line.extend_from_slice(clean(stage).as_bytes());
    line.push(b'\t');
    line.extend_from_slice(clean(message).as_bytes());
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()
}

/// Outcome of inspecting the first bytes of the helper's stderr.
#[derive(Debug, Eq, PartialEq)]
pub enum FailureSniff {
    /// More bytes are needed to decide.
    Undecided,
    /// Ordinary output.
    NotFailure,
    /// A complete failure line, plus the bytes that followed it.
    Failure {
        stage: String,
        message: String,
        rest: Vec<u8>,
    },
}

/// Classify the start of stderr. `at_eof` accepts a failure line without a newline.
pub fn sniff_failure(bytes: &[u8], at_eof: bool) -> FailureSniff {
    if bytes.len() < FAILURE_PREFIX.len() {
        return if FAILURE_PREFIX.starts_with(bytes) && !at_eof {
            FailureSniff::Undecided
        } else {
            FailureSniff::NotFailure
        };
    }
    if !bytes.starts_with(FAILURE_PREFIX) {
        return FailureSniff::NotFailure;
    }
    let body = &bytes[FAILURE_PREFIX.len()..];
    let (line, rest) = match body.iter().position(|byte| *byte == b'\n') {
        Some(end) => (&body[..end], body[end + 1..].to_vec()),
        None if at_eof => (body, Vec::new()),
        None => return FailureSniff::Undecided,
    };
    let line = String::from_utf8_lossy(line);
    let (stage, message) = line.split_once('\t').unwrap_or(("unknown", &line));
    FailureSniff::Failure {
        stage: stage.to_owned(),
        message: message.to_owned(),
        rest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan() -> HelperPlan {
        HelperPlan {
            version: PROTOCOL_VERSION,
            program: "/bin/echo".into(),
            args: vec!["hi".into()],
            parent_pid: 42,
            rlimits: RlimitPlan {
                max_tasks: Some(8),
                address_space_bytes: None,
                cpu_seconds: Some(3),
            },
            user_namespace: true,
            network_namespace: true,
            landlock: Some(LandlockPlan {
                read_only: vec!["/ro".into()],
                read_write: vec!["/rw".into()],
            }),
            seatbelt: None,
        }
    }

    #[test]
    fn framed_plan_round_trips_and_leaves_trailing_bytes() {
        let mut framed = encode_plan(&sample_plan()).expect("encode");
        framed.extend_from_slice(b"stdin for target");
        let mut cursor = io::Cursor::new(framed);
        assert_eq!(read_plan(&mut cursor).expect("decode"), sample_plan());
        let mut rest = Vec::new();
        cursor.read_to_end(&mut rest).expect("rest");
        assert_eq!(rest, b"stdin for target");
    }

    #[test]
    fn malformed_frames_are_rejected() {
        let mut bad_magic = io::Cursor::new(b"NOTMAGIC\x00\x00\x00\x00".to_vec());
        assert!(read_plan(&mut bad_magic).is_err());
        let mut oversized = PLAN_MAGIC.to_vec();
        oversized.extend_from_slice(&(MAX_PLAN_BYTES + 1).to_le_bytes());
        assert!(read_plan(&mut io::Cursor::new(oversized)).is_err());
        let mut truncated = encode_plan(&sample_plan()).expect("encode");
        truncated.truncate(truncated.len() - 3);
        assert!(read_plan(&mut io::Cursor::new(truncated)).is_err());
    }

    #[test]
    fn failure_lines_are_detected_incrementally() {
        let mut line = Vec::new();
        write_failure(&mut line, "landlock", "ABI\nunsupported").expect("write");
        assert_eq!(sniff_failure(&line[..3], false), FailureSniff::Undecided);
        assert_eq!(
            sniff_failure(b"plain error output", false),
            FailureSniff::NotFailure
        );
        let mut with_rest = line.clone();
        with_rest.extend_from_slice(b"tail");
        assert_eq!(
            sniff_failure(&with_rest, false),
            FailureSniff::Failure {
                stage: "landlock".into(),
                message: "ABI unsupported".into(),
                rest: b"tail".to_vec(),
            }
        );
        assert_eq!(
            sniff_failure(&line[..line.len() - 1], false),
            FailureSniff::Undecided
        );
        assert!(matches!(
            sniff_failure(&line[..line.len() - 1], true),
            FailureSniff::Failure { .. }
        ));
    }
}
