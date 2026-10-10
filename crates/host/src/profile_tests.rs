use super::*;

#[test]
fn filesystem_observations_distinguish_directories_files_and_unknown_paths() {
    assert_eq!(
        directory_state(Some(&env::temp_dir())),
        Probe::Available(())
    );
    assert_eq!(
        directory_state(env::current_exe().ok().as_deref()),
        Probe::Unavailable
    );
    assert_eq!(directory_state(None), Probe::Unknown);
}

#[test]
fn current_host_smoke_reports_a_versioned_profile() {
    let profile = HostProfile::current();
    println!("current host capability profile: {profile:#?}");
    assert_eq!(profile.version, PROFILE_VERSION);
    assert!(!profile.os.is_empty());
    assert!(!profile.architecture.is_empty());
    assert!(profile.ram_bytes.is_some());
    assert_eq!(profile.gpu, GpuStatus::NotProbed, "the GPU probe is opt-in");
}

#[test]
fn reports_found_and_missing_tools_from_synthetic_path() {
    let root = PathBuf::from("synthetic");
    let paths = [root.clone()];
    let found = locate_tool_with(Some(&paths), &["git"], |path| {
        Ok(path == Path::new("synthetic/git"))
    });
    assert_eq!(found, Probe::Available(root.join("git")));
    let missing = locate_tool_with(Some(&paths), &["rustc", "cargo"], |_| {
        Err(std::io::ErrorKind::NotFound)
    });
    assert_eq!(missing, Probe::Unavailable);
}

#[test]
fn reports_unknown_for_unavailable_probes() {
    assert_eq!(locate_tool(None, &["git"]), Probe::Unknown);
    let paths = [PathBuf::from("synthetic")];
    assert_eq!(
        locate_tool_with(Some(&paths), &["git"], |_| {
            Err(std::io::ErrorKind::PermissionDenied)
        }),
        Probe::Unknown
    );
    let profile = HostProfile::from_path_entries(None, None, Some(&[]));
    assert_eq!(profile.network, Probe::Unknown);
    assert_eq!(profile.gpu, GpuStatus::NotProbed);
    assert_eq!(profile.sandbox, Probe::Unknown);
    assert_eq!(profile.temporary_filesystem.writable, Probe::Unknown);
}

#[test]
fn bounded_path_search_does_not_claim_missing_after_truncation() {
    let paths = (0..=MAX_PATH_ENTRIES)
        .map(|i| PathBuf::from(format!("missing-{i}")))
        .collect::<Vec<_>>();
    let result = locate_tool_with(Some(&paths), &["git"], |_| {
        Err(std::io::ErrorKind::NotFound)
    });
    assert_eq!(result, Probe::Unknown);
}

#[test]
fn manifest_version_and_json_serialization_are_stable() {
    let profile = HostProfile::from_path_entries(None, None, Some(&[]));
    let first = serde_json::to_string(&profile).unwrap();
    let second = serde_json::to_string(&profile).unwrap();
    assert!(first.starts_with("{\"version\":2,"));
    assert_eq!(first, second);
    assert!(first.contains("\"network\":{\"state\":\"unknown\"}"));
    assert!(first.contains("\"gpu\":{\"state\":\"not_probed\"}"));
}
