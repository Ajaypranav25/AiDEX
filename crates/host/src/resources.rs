//! Memory and CPU facts from the operating system (`sysinfo`, safe API).
//!
//! These reads never run programs. Values the platform does not report are
//! `None`, never guessed.

use serde::{Deserialize, Serialize};
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind,
    System,
};

/// A point-in-time snapshot of host memory and CPU capacity.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostResources {
    /// Total physical RAM in bytes.
    pub total_ram_bytes: Option<u64>,
    /// RAM available to new allocations without swapping, in bytes.
    pub available_ram_bytes: Option<u64>,
    /// Physical CPU cores.
    pub physical_cores: Option<usize>,
    /// Logical CPUs (hardware threads).
    pub logical_cores: Option<usize>,
    /// CPU model string (for example `13th Gen Intel(R) Core(TM) i7-13650HX`).
    pub cpu_brand: Option<String>,
}

impl HostResources {
    /// Read current memory and CPU facts.
    pub fn current() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_memory(MemoryRefreshKind::nothing().with_ram())
                .with_cpu(CpuRefreshKind::nothing()),
        );
        let nonzero = |value: u64| (value > 0).then_some(value);
        let cpu_brand = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_owned())
            .filter(|brand| !brand.is_empty());
        Self {
            total_ram_bytes: nonzero(system.total_memory()),
            available_ram_bytes: nonzero(system.available_memory()),
            physical_cores: System::physical_core_count().filter(|count| *count > 0),
            logical_cores: std::thread::available_parallelism().ok().map(usize::from),
            cpu_brand,
        }
    }

    /// Cores to use for compute-bound work: physical cores when known, else
    /// half the logical CPUs, at least one.
    pub fn compute_threads(&self) -> usize {
        self.physical_cores
            .or_else(|| self.logical_cores.map(|logical| logical.div_ceil(2)))
            .unwrap_or(1)
            .max(1)
    }
}

/// Resident memory (RSS / working set) of `pids` summed, or `None` when none of
/// them could be read.
pub fn resident_memory_bytes(pids: &[u32]) -> Option<u64> {
    if pids.is_empty() {
        return None;
    }
    let targets: Vec<Pid> = pids.iter().map(|pid| Pid::from_u32(*pid)).collect();
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&targets),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let mut total = None;
    for pid in &targets {
        if let Some(process) = system.process(*pid) {
            total = Some(total.unwrap_or(0u64).saturating_add(process.memory()));
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_host_reports_memory_and_cores() {
        let resources = HostResources::current();
        println!("host resources: {resources:?}");
        let total = resources.total_ram_bytes.expect("total RAM is reported");
        let available = resources
            .available_ram_bytes
            .expect("available RAM is reported");
        assert!(available <= total);
        assert!(resources.compute_threads() >= 1);
        if let (Some(physical), Some(logical)) = (resources.physical_cores, resources.logical_cores)
        {
            assert!(physical <= logical);
        }
    }

    #[test]
    fn compute_threads_prefers_physical_cores() {
        let resources = HostResources {
            physical_cores: Some(6),
            logical_cores: Some(12),
            ..HostResources::default()
        };
        assert_eq!(resources.compute_threads(), 6);
        let logical_only = HostResources {
            logical_cores: Some(7),
            ..HostResources::default()
        };
        assert_eq!(logical_only.compute_threads(), 4);
        assert_eq!(HostResources::default().compute_threads(), 1);
    }

    #[test]
    fn resident_memory_of_this_process_is_positive() {
        let bytes = resident_memory_bytes(&[std::process::id()]).expect("own RSS");
        assert!(bytes > 0);
        assert_eq!(resident_memory_bytes(&[]), None);
    }
}
