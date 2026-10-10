//! Opt-in, best-effort GPU discovery.
//!
//! [`probe_gpus`] is the only host probe that may run a program: on Windows and
//! Linux it runs `nvidia-smi` (found through a bounded `PATH` search) with a
//! timeout and bounded output. Other vendors are not probed, so "no NVIDIA GPU"
//! is reported as unknown rather than "no GPU". On Apple Silicon the integrated
//! GPU is a platform fact and nothing is executed. Callers must request the
//! probe explicitly; `HostProfile::current()` never runs it.

use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
use crate::profile::{Probe, locate_tool, path_entries};

/// Largest `nvidia-smi` output read.
const MAX_PROBE_OUTPUT: usize = 64 * 1024;
/// Devices reported at most.
const MAX_DEVICES: usize = 16;

/// Settings for the opt-in GPU probe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuProbeOptions {
    /// Upper bound on the vendor tool's run time; it is killed afterwards.
    pub timeout: Duration,
}

impl Default for GpuProbeOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Other,
}

impl GpuVendor {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nvidia => "nvidia",
            Self::Amd => "amd",
            Self::Intel => "intel",
            Self::Apple => "apple",
            Self::Other => "other",
        }
    }
}

/// One GPU as reported by the probe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuDevice {
    pub vendor: GpuVendor,
    pub name: String,
    /// Dedicated video memory; `None` for unified memory or when unreported.
    pub vram_bytes: Option<u64>,
    pub driver_version: Option<String>,
    /// Shares system RAM with the CPU (Apple Silicon).
    pub unified_memory: bool,
}

/// What the GPU probe established.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GpuStatus {
    /// The opt-in probe was not requested.
    NotProbed,
    /// `source` reported these devices.
    Detected {
        source: String,
        devices: Vec<GpuDevice>,
    },
    /// `source` ran successfully and reported no device of its vendor.
    NotDetected { source: String },
    /// No reliable answer (tool missing, failed, or timed out).
    Unknown { reason: String },
}

impl GpuStatus {
    /// Detected devices (empty unless [`GpuStatus::Detected`]).
    pub fn devices(&self) -> &[GpuDevice] {
        match self {
            Self::Detected { devices, .. } => devices,
            _ => &[],
        }
    }

    /// A short human explanation of the status.
    pub fn summary(&self) -> String {
        match self {
            Self::NotProbed => "GPU not probed (the GPU probe is opt-in)".into(),
            Self::Detected { source, devices } => {
                let names: Vec<String> = devices
                    .iter()
                    .map(|device| match device.vram_bytes {
                        Some(vram) => format!("{} ({} MiB VRAM)", device.name, vram >> 20),
                        None => device.name.clone(),
                    })
                    .collect();
                format!("GPU detected by {source}: {}", names.join(", "))
            }
            Self::NotDetected { source } => format!("{source} reported no GPU"),
            Self::Unknown { reason } => format!("GPU unknown: {reason}"),
        }
    }
}

/// Run the best-effort GPU probe for this platform.
pub fn probe_gpus(options: &GpuProbeOptions) -> GpuStatus {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let _ = options;
        GpuStatus::Detected {
            source: "platform".into(),
            devices: vec![GpuDevice {
                vendor: GpuVendor::Apple,
                name: "Apple Silicon GPU".into(),
                vram_bytes: None,
                driver_version: None,
                unified_memory: true,
            }],
        }
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let paths = path_entries();
        match locate_tool(paths.as_deref(), &["nvidia-smi.exe", "nvidia-smi"]) {
            Probe::Available(tool) => probe_nvidia(&tool, options.timeout),
            Probe::Unavailable => GpuStatus::Unknown {
                reason: "nvidia-smi is not on PATH; other GPU vendors are not probed".into(),
            },
            Probe::Unknown => GpuStatus::Unknown {
                reason: "PATH could not be searched for nvidia-smi".into(),
            },
        }
    }
}

/// Run `nvidia-smi --query-gpu` and parse its CSV output.
pub(crate) fn probe_nvidia(tool: &Path, timeout: Duration) -> GpuStatus {
    let source = "nvidia-smi".to_owned();
    let child = Command::new(tool)
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            return GpuStatus::Unknown {
                reason: format!("cannot run nvidia-smi: {}", error.kind()),
            };
        }
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return GpuStatus::Unknown {
            reason: "nvidia-smi output was not captured".into(),
        };
    };
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        let _ = stdout
            .take(MAX_PROBE_OUTPUT as u64)
            .read_to_end(&mut output);
        output
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let output = reader.join().unwrap_or_default();
    match status {
        None => GpuStatus::Unknown {
            reason: format!(
                "nvidia-smi did not finish within {} ms",
                timeout.as_millis()
            ),
        },
        Some(status) if !status.success() => GpuStatus::Unknown {
            reason: format!("nvidia-smi failed ({status})"),
        },
        Some(_) => {
            let devices = parse_nvidia_csv(&String::from_utf8_lossy(&output));
            if devices.is_empty() {
                GpuStatus::NotDetected { source }
            } else {
                GpuStatus::Detected { source, devices }
            }
        }
    }
}

/// Parse `name, memory.total (MiB), driver_version` lines.
pub(crate) fn parse_nvidia_csv(text: &str) -> Vec<GpuDevice> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split(',').map(str::trim);
            let name = fields.next().filter(|name| !name.is_empty())?;
            let vram_bytes = fields
                .next()
                .and_then(|mib| mib.parse::<u64>().ok())
                .and_then(|mib| mib.checked_mul(1024 * 1024));
            let driver_version = fields
                .next()
                .filter(|driver| !driver.is_empty() && !driver.contains("N/A"))
                .map(str::to_owned);
            Some(GpuDevice {
                vendor: GpuVendor::Nvidia,
                name: name.chars().take(128).collect(),
                vram_bytes,
                driver_version,
                unified_memory: false,
            })
        })
        .take(MAX_DEVICES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvidia_csv_lines_become_devices() {
        let devices = parse_nvidia_csv(
            "NVIDIA GeForce RTX 3050 6GB Laptop GPU, 6144, 595.97\nTesla T4, [N/A], [N/A]\n\n",
        );
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].vendor, GpuVendor::Nvidia);
        assert_eq!(devices[0].name, "NVIDIA GeForce RTX 3050 6GB Laptop GPU");
        assert_eq!(devices[0].vram_bytes, Some(6144 * 1024 * 1024));
        assert_eq!(devices[0].driver_version.as_deref(), Some("595.97"));
        assert_eq!(devices[1].vram_bytes, None);
        assert_eq!(devices[1].driver_version, None);
        assert!(parse_nvidia_csv("").is_empty());
    }

    #[test]
    fn statuses_serialize_with_a_state_tag_and_explain_themselves() {
        let json = serde_json::to_string(&GpuStatus::NotProbed).unwrap();
        assert_eq!(json, r#"{"state":"not_probed"}"#);
        let unknown = GpuStatus::Unknown {
            reason: "nvidia-smi is not on PATH".into(),
        };
        assert!(unknown.summary().starts_with("GPU unknown"));
        assert!(unknown.devices().is_empty());
    }

    #[test]
    fn a_missing_tool_is_unknown_not_absent() {
        let status = probe_nvidia(
            Path::new("aidex-test-missing-nvidia-smi-binary"),
            Duration::from_secs(1),
        );
        assert!(matches!(status, GpuStatus::Unknown { .. }), "{status:?}");
    }

    #[test]
    fn opt_in_probe_runs_and_reports_a_status() {
        let status = probe_gpus(&GpuProbeOptions::default());
        println!("gpu probe: {}", status.summary());
        assert_ne!(status, GpuStatus::NotProbed);
    }
}
