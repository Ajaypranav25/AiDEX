//! Capability negotiation reports exact enforcement and fails closed.

mod common;

use std::time::Duration;

use aidex_sandbox::{
    BackendKind, Enforcement, HelperStatus, Mount, NetworkMode, Requirement, ResourceLimits,
    Sandbox, SandboxError, SandboxLevel, requirements_for,
};
use common::{request, sandbox, workspace};

#[test]
fn probe_reports_every_requirement_once_in_order() {
    let capabilities = sandbox().capabilities().clone();
    let reported: Vec<Requirement> = capabilities
        .capabilities
        .iter()
        .map(|capability| capability.requirement)
        .collect();
    assert_eq!(reported, Requirement::ALL.to_vec());
    assert!(matches!(
        capabilities.helper,
        HelperStatus::Available { .. }
    ));
    assert!(capabilities.max_level.is_some());
    let json = serde_json::to_value(&capabilities).expect("serialize");
    assert_eq!(json["version"], 1);
    assert_eq!(json["capabilities"].as_array().map(Vec::len), Some(11));
}

#[cfg(windows)]
#[test]
fn windows_reports_brokered_maximum_and_unenforced_confinement() {
    let capabilities = sandbox().capabilities().clone();
    assert_eq!(capabilities.backend, BackendKind::WindowsJobObject);
    assert_eq!(capabilities.max_level, Some(SandboxLevel::Brokered));
    for requirement in [
        Requirement::FilesystemReadConfinement,
        Requirement::FilesystemWriteConfinement,
        Requirement::NetworkDeny,
        Requirement::NetworkAllowlist,
        Requirement::EphemeralEnvironment,
    ] {
        assert!(
            matches!(
                capabilities.enforcement(requirement),
                Enforcement::NotEnforced { .. }
            ),
            "{requirement}"
        );
    }
    for requirement in [
        Requirement::ProcessTreeTermination,
        Requirement::WallTimeout,
        Requirement::EnvIsolation,
    ] {
        assert!(
            matches!(
                capabilities.enforcement(requirement),
                Enforcement::Enforced { .. }
            ),
            "{requirement}"
        );
    }
    for requirement in [
        Requirement::ProcessCountLimit,
        Requirement::MemoryLimit,
        Requirement::CpuTimeLimit,
    ] {
        assert!(
            matches!(
                capabilities.enforcement(requirement),
                Enforcement::Watchdog { .. }
            ),
            "{requirement}"
        );
    }
}

#[cfg(windows)]
#[test]
fn restricted_on_windows_fails_closed_listing_filesystem_and_network() {
    let workspace = workspace();
    let request = request(SandboxLevel::Restricted, &workspace).with_network(NetworkMode::Offline);
    let error = sandbox()
        .negotiate(&request)
        .expect_err("restricted must fail on Windows");
    assert_eq!(
        error,
        SandboxError::Unsupported {
            level: SandboxLevel::Restricted,
            missing: vec![
                Requirement::FilesystemReadConfinement,
                Requirement::FilesystemWriteConfinement,
                Requirement::NetworkDeny,
            ],
        }
    );
    assert_eq!(error.kind(), "sandbox.unsupported");
    // Nothing was prepared for a launch that will never happen.
    assert!(!workspace.scratch.join("tmp").exists());
}

#[cfg(windows)]
#[test]
fn offline_network_is_never_silently_dropped_on_windows() {
    let workspace = workspace();
    let request = request(SandboxLevel::Brokered, &workspace).with_network(NetworkMode::Offline);
    assert_eq!(
        sandbox().negotiate(&request).expect_err("offline"),
        SandboxError::Unsupported {
            level: SandboxLevel::Brokered,
            missing: vec![Requirement::NetworkDeny],
        }
    );
}

#[test]
fn direct_mode_requires_the_explicit_opt_in() {
    let workspace = workspace();
    let mut denied = request(SandboxLevel::Direct, &workspace);
    denied.allow_direct = false;
    let error = sandbox()
        .negotiate(&denied)
        .expect_err("direct without opt-in");
    assert_eq!(error, SandboxError::DirectNotAllowed);
    assert_eq!(error.code(), aidex_core::ServiceErrorCode::PermissionDenied);

    let plan = sandbox()
        .negotiate(&request(SandboxLevel::Direct, &workspace))
        .expect("direct with opt-in");
    assert!(plan.direct_mode);
    assert_eq!(plan.level, SandboxLevel::Direct);
    assert!(plan.applies(Requirement::EnvIsolation));
    assert!(plan.applies(Requirement::ProcessTreeTermination));
    assert!(!plan.applies(Requirement::FilesystemWriteConfinement));
}

#[test]
fn allowlist_network_and_ephemeral_level_always_fail_closed() {
    let workspace = workspace();
    let allowlist = request(SandboxLevel::Brokered, &workspace)
        .with_network(NetworkMode::Allowlist(vec!["github.com".into()]));
    match sandbox().negotiate(&allowlist) {
        Err(SandboxError::Unsupported { missing, .. }) => {
            assert!(missing.contains(&Requirement::NetworkAllowlist));
        }
        other => panic!("allowlist must fail closed, got {other:?}"),
    }
    let ephemeral = request(SandboxLevel::Ephemeral, &workspace);
    match sandbox().negotiate(&ephemeral) {
        Err(SandboxError::Unsupported { level, missing }) => {
            assert_eq!(level, SandboxLevel::Ephemeral);
            assert!(missing.contains(&Requirement::EphemeralEnvironment));
        }
        other => panic!("ephemeral must fail closed, got {other:?}"),
    }
}

#[test]
fn plan_states_exactly_what_is_enforced() {
    let workspace = workspace();
    let request = request(SandboxLevel::Brokered, &workspace).with_limits(ResourceLimits {
        wall_timeout: Some(Duration::from_secs(30)),
        max_cpu_time: Some(Duration::from_secs(10)),
        ..ResourceLimits::default()
    });
    let plan = sandbox().negotiate(&request).expect("plan");
    assert_eq!(
        requirements_for(&request),
        vec![
            Requirement::ProcessTreeTermination,
            Requirement::CpuTimeLimit,
            Requirement::WallTimeout,
            Requirement::EnvIsolation,
        ]
    );
    assert_eq!(plan.controls.len(), Requirement::ALL.len());
    for control in &plan.controls {
        let required = requirements_for(&request).contains(&control.requirement);
        assert_eq!(control.required, required, "{}", control.requirement);
        if required {
            assert!(control.enforcement.satisfies(control.requirement));
        } else {
            assert!(matches!(
                control.enforcement,
                Enforcement::NotEnforced { .. }
            ));
        }
    }
    let memory = plan
        .control(Requirement::MemoryLimit)
        .expect("memory control");
    assert_eq!(
        memory.enforcement,
        Enforcement::NotEnforced {
            reason: "not requested".into()
        }
    );
    let json = serde_json::to_value(&plan).expect("serialize plan");
    assert_eq!(json["level"], "brokered");
    assert_eq!(json["limits"]["wall_timeout"], 30_000);
    assert!(
        json["plan_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("plan_"))
    );
    // The scratch mount is explicit in the plan.
    assert!(
        plan.mounts
            .iter()
            .any(|mount| mount.host_path.ends_with("scratch"))
    );
}

#[test]
fn malformed_requests_are_rejected() {
    let workspace = workspace();
    let sandbox = sandbox();
    let invalid = |request| match sandbox.negotiate(&request) {
        Err(SandboxError::InvalidRequest(message)) => message,
        other => panic!("expected InvalidRequest, got {other:?}"),
    };

    let mut outside_cwd = request(SandboxLevel::Brokered, &workspace);
    outside_cwd.cwd = workspace.outside.clone();
    assert!(invalid(outside_cwd).contains("not inside any mount"));

    let nested =
        request(SandboxLevel::Brokered, &workspace).with_mount(Mount::read_only(&workspace.work));
    assert!(invalid(nested).contains("nested read-only"));

    let relative =
        request(SandboxLevel::Brokered, &workspace).with_mount(Mount::read_only("relative"));
    assert!(invalid(relative).contains("absolute"));

    let mut secret_allowlist = request(SandboxLevel::Brokered, &workspace);
    secret_allowlist.env_allowlist.push("GITHUB_TOKEN".into());
    assert!(invalid(secret_allowlist).contains("credential"));

    let managed = request(SandboxLevel::Brokered, &workspace).with_env("TEMP", "C:\\elsewhere");
    assert!(invalid(managed).contains("managed"));

    let zero = request(SandboxLevel::Brokered, &workspace).with_limits(ResourceLimits {
        max_processes: Some(0),
        ..ResourceLimits::default()
    });
    assert!(invalid(zero).contains("max_processes"));
}

#[test]
fn plans_and_requests_never_expose_environment_values() {
    let workspace = workspace();
    let secret = "plan-secret-value-7d1e";
    let request = request(SandboxLevel::Brokered, &workspace).with_env("TOOL_TOKEN", secret);
    assert!(!format!("{request:?}").contains(secret));
    let plan = sandbox().negotiate(&request).expect("plan");
    let json = serde_json::to_string(&plan).expect("serialize");
    assert!(json.contains("TOOL_TOKEN"));
    assert!(!json.contains(secret));
    assert!(!format!("{plan:?}").contains(secret));
}

#[test]
fn missing_helper_fails_closed() {
    let workspace = workspace();
    let sandbox = Sandbox::new(workspace.dir.path().join("no-such-helper"));
    let capabilities = sandbox.capabilities();
    assert!(matches!(capabilities.helper, HelperStatus::Missing { .. }));
    assert_eq!(capabilities.max_level, None);
    match sandbox.negotiate(&request(SandboxLevel::Brokered, &workspace)) {
        Err(SandboxError::HelperUnavailable(_)) => {}
        other => panic!("expected HelperUnavailable, got {other:?}"),
    }
}

#[test]
fn plans_are_bound_to_the_negotiating_helper() {
    let workspace = workspace();
    let copy = workspace
        .dir
        .path()
        .join(format!("helper-copy{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(common::HELPER, &copy).expect("copy helper");
    let plan = Sandbox::new(&copy)
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let command = common::shell(&plan, "exit 0");
    match sandbox().spawn(&plan, command) {
        Err(SandboxError::PlanMismatch(_)) => {}
        other => panic!("expected PlanMismatch, got {other:?}"),
    }
}

#[test]
fn relative_programs_must_be_resolved_first() {
    let workspace = workspace();
    let sandbox = sandbox();
    let plan = sandbox
        .negotiate(&request(SandboxLevel::Brokered, &workspace))
        .expect("plan");
    let relative = aidex_sandbox::CommandSpec::new("relative-program");
    assert!(matches!(
        sandbox.spawn(&plan, relative),
        Err(SandboxError::ProgramNotFound(_))
    ));
    assert!(matches!(
        plan.resolve_program("definitely-not-a-real-program-aidex"),
        Err(SandboxError::ProgramNotFound(_))
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_maximum_level_follows_landlock_availability() {
    let capabilities = sandbox().capabilities().clone();
    assert_eq!(capabilities.backend, BackendKind::LinuxLandlock);
    let landlock = capabilities
        .enforcement(Requirement::FilesystemWriteConfinement)
        .satisfies(Requirement::FilesystemWriteConfinement);
    let expected = if landlock {
        SandboxLevel::Restricted
    } else {
        SandboxLevel::Brokered
    };
    assert_eq!(
        capabilities.max_level,
        Some(expected),
        "{:?}",
        capabilities.facts
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_restricted_requires_a_read_only_root_mount() {
    let workspace = workspace();
    let capabilities = sandbox().capabilities().clone();
    assert_eq!(capabilities.backend, BackendKind::MacosSeatbelt);
    assert_eq!(capabilities.max_level, Some(SandboxLevel::Brokered));
    match sandbox().negotiate(&request(SandboxLevel::Restricted, &workspace)) {
        Err(SandboxError::Unsupported { missing, .. }) => {
            assert!(missing.contains(&Requirement::FilesystemReadConfinement));
        }
        other => panic!("expected Unsupported, got {other:?}"),
    }
}
