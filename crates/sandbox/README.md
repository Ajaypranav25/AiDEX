# aidex-sandbox

Fail-closed OS process isolation for processes AiDEX launches on behalf of agents and tools (spec §12, §48, §58-60, §90). The execution broker (later wave) decides *whether* something may run; this crate decides *how* it is contained and refuses to run it when the requested containment cannot be enforced.

## Flow

```rust
let sandbox = Sandbox::locate();                 // helper next to the current executable
let caps = sandbox.capabilities();               // probed once, serializable (doctor hook)
let request = SandboxRequest::new(SandboxLevel::Brokered, &workspace, &scratch)
    .with_mount(Mount::read_write(&workspace))
    .with_network(NetworkMode::Normal)
    .with_limits(ResourceLimits { wall_timeout: Some(Duration::from_secs(600)), ..Default::default() });
let plan = sandbox.negotiate(&request)?;         // EXACT enforcement, or Err(Unsupported { missing })
let program = plan.resolve_program("cargo")?;    // absolute path; no PATH lookup inside the child
let process = sandbox.spawn(&plan, CommandSpec::new(program).args(["test"]))?;
let status = process.wait()?;                    // ExitStatusInfo { code, signal, killed_by, .. }
let output = process.take_output();              // bounded, truncation flagged
sandbox.registry().shutdown(StopReason::EmergencyStop); // every tree, with a report
```

`aidex_sandbox::probe()` and `aidex_sandbox::negotiate(&request)` are free-function shortcuts using the default helper location; `aidex-host`/doctor can serialize `probe()` directly (`BackendCapabilities` is versioned, `version: 1`).

## Public API

| Item | Purpose |
| --- | --- |
| `SandboxLevel` | `Direct` (0), `Brokered` (1), `Restricted` (2), `Ephemeral` (3), ordered. |
| `SandboxRequest` | level, `mounts: Vec<Mount { host_path, access: ReadOnly \| ReadWrite }>`, `cwd` (must be inside a mount), `scratch_dir`, `env_allowlist`, explicit `env` additions, `network: NetworkMode { Offline, Allowlist(domains), Normal, Unrestricted }`, `limits: ResourceLimits { max_processes, max_memory_bytes, max_cpu_time, wall_timeout, max_output_bytes }`, `allow_direct`. `Debug` redacts env values; the type is deliberately not serializable. |
| `Requirement` | `process_tree_termination`, `process_count_limit`, `memory_limit`, `cpu_time_limit`, `wall_timeout`, `env_isolation`, `filesystem_read_confinement`, `filesystem_write_confinement`, `network_deny`, `network_allowlist`, `ephemeral_environment`. |
| `Enforcement` | `Enforced { mechanism }`, `Watchdog { mechanism, interval_ms }` (sampled, tree killed on violation; accepted only for resource limits), `NotEnforced { reason }` (backend does not implement it), `Unavailable { reason }` (host lacks the facility). |
| `BackendCapabilities` | Per-requirement `Capability { requirement, enforcement, limitations }`, `backend`, `helper` status, `max_level`, probe `facts`. |
| `requirements_for(&request)` / `baseline_requirements(level)` | Requirement derivation used by negotiation. |
| `Sandbox::negotiate` | Returns `EnforcementPlan` or fails with `SandboxError::Unsupported { level, missing }`, `DirectNotAllowed`, `InvalidRequest`, or `HelperUnavailable`. Never downgrades. |
| `EnforcementPlan` | Serializable audit record (for a later `sandbox.negotiated` event): `plan_id`, `negotiated_at`, `backend`, `level`, `direct_mode`, one `PlannedControl { requirement, required, enforcement, limitations }` per requirement, canonical `mounts` (including the implicit read-write scratch mount), `cwd`, `temp_dir`, `home_dir`, `network`, `limits`, and environment variable **names** only. |
| `Sandbox::spawn(&plan, CommandSpec { program, args, stdin })` | Returns a `ManagedProcess` only after the helper confirmed every planned restriction; otherwise `SandboxError::EnforcementFailed { stage, message }` and the target never ran. |
| `ManagedProcess` | `id()` (`proc_<ULID>`), `pid()`, `plan()`, `wait()`, `wait_timeout()`, `try_wait()`, `is_running()`, `kill_tree()`, `output_so_far()`, `take_output()`, `member_pids()`. Dropping it kills the tree. |
| `ExitStatusInfo` | `code`, `signal` (Unix), `killed_by: Option<KillReason>` (`Requested`, `Dropped`, `WallTimeout`, `LimitExceeded(Requirement)`, `Stopped(StopReason)`, `HandshakeFailed`), `elapsed`, `output_complete`. |
| `ProcessRegistry` | Clone-shared registry of live trees: `terminate_all(StopReason)` / `terminate_all_within(reason, grace)` return a `TerminationReport { terminated, already_exited, failed }`; `shutdown(reason)` also closes the registry (`RegistryClosed` for new spawns) until `reopen()`. `StopReason`: `Cancelled`, `DaemonShutdown`, `EmergencyStop`, `DriveRemoved`. |
| `resolve_program`, `is_secret_like`, `DEFAULT_ENV_ALLOWLIST`, `MANAGED_ENV_VARS` | Environment helpers. |
| `SandboxError::code()` / `kind()` | Stable `aidex_core::ServiceErrorCode` and a finer `sandbox.*` identifier. |

## Levels and requirements

Every request requires `process_tree_termination` and `env_isolation`. `Restricted` adds read and write filesystem confinement; `Ephemeral` adds `ephemeral_environment`, which no backend implements, so it always fails closed. Requested limits add `process_count_limit`, `memory_limit`, `cpu_time_limit`, and `wall_timeout`; `NetworkMode::Offline` adds `network_deny` and `Allowlist` adds `network_allowlist` at **any** level. Network `Normal` and `Unrestricted` apply no OS restriction (the broker governs `Normal`).

**Direct mode (level 0)** is permitted only with `allow_direct: true`, an explicit user choice the caller must surface. It runs the program as the current user without filesystem or network confinement, but still with a cleared environment, bounded output, tree termination, and any limits the request asks for (an `Offline` request still fails if network denial is unavailable). The plan records `direct_mode: true`.

## Enforcement matrix

| Requirement | Windows (Job Object backend) | Linux (Landlock backend) | macOS (Seatbelt backend) |
| --- | --- | --- | --- |
| Process tree termination | Enforced: Job Object with `KILL_ON_JOB_CLOSE`; the helper joins the job before starting the target, so no descendant can predate the job | Enforced: `SIGKILL` to the process group; target dies with its parent thread (`PR_SET_PDEATHSIG`). Limitation: `setsid()`/`setpgid()` escapes the group | Enforced: process-group `SIGKILL`. Limitations: `setsid()` escapes; no parent-death signal |
| Process count | Watchdog (25 ms): job membership sampled, tree killed above the limit; creation is not refused. Helper and its `conhost.exe` are not counted | Enforced: `RLIMIT_NPROC` inside a private user namespace (counts threads). Unavailable without unprivileged user namespaces | Not enforced (`RLIMIT_NPROC` is per user) |
| Memory | Watchdog: per-process private commit | Enforced: `RLIMIT_AS` per process (virtual address space) | Not enforced (macOS ignores `RLIMIT_AS`) |
| CPU time | Watchdog: per-process user+kernel time | Enforced: `RLIMIT_CPU` per process (`SIGKILL`) | Enforced: `RLIMIT_CPU` per process |
| Wall timeout | Enforced by the parent; whole job killed | Enforced by the parent; group killed | Enforced by the parent; group killed |
| Environment isolation | Enforced: cleared environment block plus allowlist | Enforced | Enforced |
| Filesystem read confinement | **Not enforced** (needs restricted tokens/AppContainer) | Enforced: Landlock, ABI v3 rights as a hard requirement (Linux 6.2+); mounts plus read-only system paths | Not enforced; satisfied only when the request mounts `/` read-only (all reads allowed) |
| Filesystem write confinement | **Not enforced** | Enforced: Landlock | Enforced: `sandbox-exec` profile denies `file-write*` outside read-write mounts |
| Network deny | **Not enforced** (needs AppContainer/WFP) | Enforced: private network namespace (loopback down too). Unavailable without unprivileged user namespaces | Enforced: `(deny network*)`. Limitation: mach services remain reachable |
| Network allowlist | Not enforced | Not enforced | Not enforced |
| Ephemeral environment | Not enforced | Not enforced | Not enforced |
| Maximum level | **Brokered** | **Restricted** when Landlock ABI ≥ 3 is enforceable, else Brokered | **Brokered** (Restricted only with a read-only `/` mount) |

The table states design intent; the probe on each host reports what is actually available (`BackendCapabilities.facts`, for example `landlock_abi=4`, `user_namespace=true`). Validation status of this PR: the Windows backend and the Linux backend (WSL2 Ubuntu 24.04, kernel 6.6, Landlock ABI 4, user and network namespaces available) pass the full test suite locally. The macOS backend compiles under `cfg` and is exercised only by CI; until its tests pass there it must not be described as supported.

Windows note: Level 1 on Windows contains runaway and accidental behaviour, not hostile native code. A process running as the same user can open other processes of that user, including the daemon. Restricted and Ephemeral requests fail closed on Windows because restricted tokens, AppContainer, and WFP filters are not reachable through a safe Rust API, and this workspace forbids `unsafe` code. For the same reason the safe `win32job` 2.0 API exposes only `KILL_ON_JOB_CLOSE`, so the process, memory, and CPU limits are watchdog-enforced rather than kernel-enforced; plans say `watchdog` for them.

## Environment

The child starts from an empty environment. The default allowlist copies only `PATH`, `PATHEXT`, `SystemRoot`, `SystemDrive`, `windir`, `ComSpec`, `NUMBER_OF_PROCESSORS`, `PROCESSOR_ARCHITECTURE`, `OS`, `LANG` (Windows) or `PATH`, `LANG`, `LC_ALL`, `LC_CTYPE`, `TZ` (Unix). `TEMP`/`TMP`/`TMPDIR` point at `<scratch>/tmp` and `HOME` (plus `USERPROFILE`, `APPDATA`, `LOCALAPPDATA` on Windows) at `<scratch>/home`; these managed names cannot be inherited or overridden. Allowlisting a credential-like name (`*_TOKEN`, `*_KEY`, `*SECRET*`, `AWS_*`, `SSH_*`, ...) is rejected: the broker passes a resolved secret through `SandboxRequest::env` for the one tool that needs it. Environment values never appear in plans, `Debug` output, serialized records, or the helper protocol.

## Helper binary

`aidex-sandbox-exec` (a `[[bin]]` of this crate) must ship next to `aidexd`; `Sandbox::locate()` looks beside the current executable (and in the parent of a Cargo `deps/` directory). The parent starts it with the final child environment and working directory, places it in the container (Windows job / Unix process group), then sends a framed JSON plan on stdin. The helper applies namespaces, rlimits, Landlock, or the Seatbelt profile to itself, writes a ready frame on stdout, and starts (Windows) or `exec`s (Unix) the target. Any failure is reported as a single failure line and exit code 125; `spawn` then returns `EnforcementFailed` and nothing ran unconfined. A missing helper makes `max_level` `None` and every negotiation fail with `HelperUnavailable`.

## Limitations and follow-ups

- Windows: no filesystem or network confinement; limits are watchdog-enforced (overshoot up to one 25 ms interval). AppContainer/restricted-token support needs a vetted safe binding.
- Linux: process-group termination is escapable via `setsid()`; a PID namespace or delegated cgroup would close that. `RLIMIT_AS` bounds address space, not RSS. Network denial and the process-count limit need unprivileged user namespaces (often restricted by AppArmor on Ubuntu 24.04 hosts, where they fail closed). Signal and abstract-socket scoping require Landlock ABI 6.
- macOS: `sandbox-exec` is deprecated by Apple but still shipped; reads are not confined; no process-count or memory limit.
- No network allowlist proxy and no ephemeral (container/VM/overlay) backend yet; copy-on-write workspaces live in `aidex-workspace`.
- Programs, arguments, and mount paths must be valid UTF-8 (they travel in the JSON helper plan).
- The daemon should make itself non-dumpable and stop the registry on shutdown/drive removal; that wiring belongs to `aidexd` and the broker.
