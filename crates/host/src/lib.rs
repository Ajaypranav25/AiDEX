//! Read-only, bounded inspection of the current process host environment.
//!
//! [`HostProfile`] is a versioned capability manifest. Default discovery never
//! invokes programs, opens network connections, or writes files: tool
//! availability is inferred from executable files visible in `PATH`, and
//! memory/CPU facts come from the operating system through `sysinfo`. The only
//! probe that runs a program is the opt-in GPU probe ([`probe_gpus`]), which a
//! caller must request explicitly.

mod gpu;
mod profile;
mod resources;

pub use gpu::{GpuDevice, GpuProbeOptions, GpuStatus, GpuVendor, probe_gpus};
pub use profile::{
    DirectoryFacts, HostInfo, HostProbeOptions, HostProfile, NamedTool, PROFILE_VERSION, Probe,
    RustToolchain,
};
pub use resources::{HostResources, resident_memory_bytes};
