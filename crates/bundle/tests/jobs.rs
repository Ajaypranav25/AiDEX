mod common;

use aidex_bundle::{InstallJobs, InstallSource, JobState, Target, VerifyScope, resolve_active};
use aidex_drive::DrivePaths;
use std::time::Duration;

#[test]
fn a_background_install_reports_success_only_after_a_verified_pointer_switch() {
    let drive = common::temp_drive();
    let (_source, bundle) = common::bundle(common::host(), "0.1.0", "jobs");
    let jobs = InstallJobs::default();
    let id = jobs
        .start(InstallSource::Directory(bundle), drive.path().to_path_buf())
        .unwrap();
    let finished = jobs.wait(&id, Duration::from_secs(20)).unwrap();
    assert_eq!(finished.state, JobState::Completed);
    assert_eq!(finished.completed_steps, 6);
    assert!(finished.report.is_some());
    let paths = DrivePaths::open(drive.path()).unwrap();
    assert_eq!(
        resolve_active(
            &paths,
            Target::host().unwrap(),
            VerifyScope::All,
            &common::unsigned()
        )
        .unwrap()
        .bundle
        .manifest
        .version,
        "0.1.0"
    );
    assert!(drive.path().join(".aidex/runtime/install-jobs").is_dir());
    let history = InstallJobs::history(drive.path()).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].id, id);
    assert_eq!(history[0].state, JobState::Completed);
}

#[test]
fn a_corrupt_bundle_fails_without_replacing_the_active_application() {
    let drive = common::temp_drive();
    let (_source, bundle) = common::bundle(common::host(), "0.1.0", "bad");
    std::fs::write(
        bundle.join(aidex_bundle::layout::bin_path(
            aidex_bundle::layout::CLI_BINARY,
            common::host(),
        )),
        "tampered",
    )
    .unwrap();
    let jobs = InstallJobs::default();
    let id = jobs
        .start(InstallSource::Directory(bundle), drive.path().to_path_buf())
        .unwrap();
    let failed = jobs.wait(&id, Duration::from_secs(20)).unwrap();
    assert_eq!(failed.state, JobState::Failed);
    assert!(failed.report.is_none());
    assert!(failed.error.as_ref().unwrap().contains("integrity"));
    assert!(common::pointer_bytes(drive.path()).is_none());
}

#[test]
fn an_unknown_job_never_cancels_an_unrelated_install() {
    let jobs = InstallJobs::default();
    assert!(jobs.cancel("unknown").is_err());
    assert!(jobs.status("unknown").is_err());
}
