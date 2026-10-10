//! Resource limits: bounded output, process count, memory, and CPU time.

mod common;

use std::time::Duration;

use aidex_sandbox::{
    Enforcement, KillReason, Requirement, ResourceLimits, SandboxError, SandboxLevel,
};
use common::{finish, marker_gone, processes_with_marker, request, sandbox, shell, workspace};

fn marker(index: u64) -> String {
    (6_000_000 + u64::from(std::process::id() % 100_000) * 100 + index).to_string()
}

fn limited(limits: ResourceLimits) -> ResourceLimits {
    ResourceLimits {
        wall_timeout: Some(Duration::from_secs(60)),
        ..limits
    }
}

#[test]
fn output_capture_is_bounded_and_truncation_is_flagged() {
    let workspace = workspace();
    let sandbox = sandbox();
    let request = request(SandboxLevel::Brokered, &workspace).with_limits(ResourceLimits {
        max_output_bytes: 1000,
        ..ResourceLimits::default()
    });
    let plan = sandbox.negotiate(&request).expect("plan");
    let command = if cfg!(windows) {
        common::powershell(
            &plan,
            "[Console]::Out.Write('x' * 200000); [Console]::Out.Flush()",
        )
    } else {
        shell(&plan, "head -c 200000 /dev/zero | tr '\\000' x")
    };
    let process = sandbox.spawn(&plan, command).expect("spawn");
    let status = finish(&process);
    assert!(
        status.success(),
        "{status:?} {}",
        process.output_so_far().stderr.text()
    );
    let output = process.take_output();
    assert_eq!(output.stdout.bytes.len(), 1000);
    assert!(output.stdout.bytes.iter().all(|byte| *byte == b'x'));
    assert!(output.stdout.truncated);
    assert_eq!(output.stdout.total_bytes, 200_000);
    assert_eq!(output.stdout.dropped_bytes, 199_000);
    assert!(!output.stderr.truncated);
    // Taking frees the buffer; the counters stay.
    let again = process.output_so_far();
    assert!(again.stdout.bytes.is_empty());
    assert_eq!(again.stdout.total_bytes, 200_000);
}

#[test]
fn process_count_limit_stops_fork_bombs() {
    let workspace = workspace();
    let sandbox = sandbox();
    let request =
        request(SandboxLevel::Brokered, &workspace).with_limits(limited(ResourceLimits {
            max_processes: Some(4),
            ..ResourceLimits::default()
        }));
    let plan = match sandbox.negotiate(&request) {
        Ok(plan) => plan,
        Err(SandboxError::Unsupported { missing, .. }) => {
            // Fail closed where the host cannot bound the process count.
            assert_eq!(missing, vec![Requirement::ProcessCountLimit]);
            assert!(
                !sandbox
                    .capabilities()
                    .enforcement(Requirement::ProcessCountLimit)
                    .satisfies(Requirement::ProcessCountLimit)
            );
            return;
        }
        Err(error) => panic!("unexpected error: {error}"),
    };
    let marker = marker(1);
    let script = if cfg!(windows) {
        format!(
            "(for /l %i in (1,1,12) do start /b ping -n {marker} 127.0.0.1 >nul) & ping -n {marker} 127.0.0.1 >nul & echo survived"
        )
    } else {
        format!(
            "for i in 1 2 3 4 5 6 7 8 9 10 11 12; do sleep {marker} & echo $! >> forks.txt; done; echo survived; sleep {marker}"
        )
    };
    let process = sandbox.spawn(&plan, shell(&plan, &script)).expect("spawn");
    match plan
        .control(Requirement::ProcessCountLimit)
        .map(|c| &c.enforcement)
    {
        Some(Enforcement::Watchdog { .. }) => {
            // Windows: the job is killed as soon as the count exceeds the limit.
            let status = finish(&process);
            assert_eq!(
                status.killed_by,
                Some(KillReason::LimitExceeded(Requirement::ProcessCountLimit)),
                "{status:?}"
            );
            assert!(!process.output_so_far().stdout.text().contains("survived"));
        }
        Some(Enforcement::Enforced { .. }) => {
            // Linux: fork() fails once RLIMIT_NPROC is reached (the shell counts as
            // one task, so at most three background children can exist).
            let status = finish(&process);
            let forks = common::read(&workspace.work.join("forks.txt"))
                .lines()
                .count();
            assert!(
                (1..4).contains(&forks),
                "process limit bypassed: {forks} forks, {status:?}"
            );
            let stderr = process.output_so_far().stderr.text().to_lowercase();
            assert!(
                stderr.contains("fork") || stderr.contains("resource"),
                "{stderr}"
            );
        }
        other => panic!("unexpected enforcement {other:?}"),
    }
    assert!(
        marker_gone(&marker),
        "survivors: {:?}",
        processes_with_marker(&marker)
    );
}

#[test]
fn memory_limit_stops_large_allocations() {
    let workspace = workspace();
    let sandbox = sandbox();
    let request =
        request(SandboxLevel::Brokered, &workspace).with_limits(limited(ResourceLimits {
            max_memory_bytes: Some(256 * 1024 * 1024),
            ..ResourceLimits::default()
        }));
    let plan = match sandbox.negotiate(&request) {
        Ok(plan) => plan,
        Err(SandboxError::Unsupported { missing, .. }) => {
            assert_eq!(missing, vec![Requirement::MemoryLimit]);
            return;
        }
        Err(error) => panic!("unexpected error: {error}"),
    };
    let command = if cfg!(windows) {
        common::powershell(
            &plan,
            "$block = New-Object byte[] (1024MB); $block[0] = 1; Start-Sleep -Seconds 30; Write-Output survived",
        )
    } else {
        shell(
            &plan,
            "dd if=/dev/zero of=/dev/null bs=600M count=1 && echo survived",
        )
    };
    let process = sandbox.spawn(&plan, command).expect("spawn");
    let status = finish(&process);
    assert!(!status.success(), "{status:?}");
    assert!(!process.output_so_far().stdout.text().contains("survived"));
    if cfg!(windows) {
        assert_eq!(
            status.killed_by,
            Some(KillReason::LimitExceeded(Requirement::MemoryLimit)),
            "{status:?}"
        );
    }
}

#[test]
fn cpu_time_limit_stops_busy_loops() {
    let workspace = workspace();
    let sandbox = sandbox();
    let request =
        request(SandboxLevel::Brokered, &workspace).with_limits(limited(ResourceLimits {
            max_cpu_time: Some(Duration::from_secs(2)),
            ..ResourceLimits::default()
        }));
    let plan = sandbox
        .negotiate(&request)
        .expect("cpu time limits are enforced on every backend");
    let command = if cfg!(windows) {
        common::powershell(&plan, "while ($true) { }")
    } else {
        shell(&plan, "while :; do :; done")
    };
    let process = sandbox.spawn(&plan, command).expect("spawn");
    let status = finish(&process);
    assert!(!status.success(), "{status:?}");
    assert!(status.elapsed < Duration::from_secs(45), "{status:?}");
    if cfg!(windows) {
        assert_eq!(
            status.killed_by,
            Some(KillReason::LimitExceeded(Requirement::CpuTimeLimit)),
            "{status:?}"
        );
    } else {
        // RLIMIT_CPU: SIGKILL (9) at the hard limit, or SIGXCPU (24 on Linux, 24 on macOS).
        assert!(matches!(status.signal, Some(9) | Some(24)), "{status:?}");
        assert_eq!(status.killed_by, None);
    }
}
