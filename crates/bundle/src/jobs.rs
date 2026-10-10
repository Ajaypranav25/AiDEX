//! Observable installation jobs for setup clients. All file operations remain in
//! the verified, atomic bundle installer; this service owns job lifecycle only.

use crate::{BundleError, InstallSource, InstallStep, install_with};
use aidex_core::EntityId;
use aidex_drive::DrivePaths;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallJob {
    pub id: String,
    pub drive_root: PathBuf,
    pub state: JobState,
    pub step: String,
    pub completed_steps: usize,
    pub total_steps: usize,
    pub report: Option<serde_json::Value>,
    pub error: Option<String>,
}

struct Entry {
    job: InstallJob,
    cancel: bool,
    journal: PathBuf,
}
#[derive(Default)]
struct Shared {
    jobs: Mutex<HashMap<String, Entry>>,
    changed: Condvar,
}

#[derive(Clone, Default)]
pub struct InstallJobs {
    shared: Arc<Shared>,
}

fn failure(message: impl Into<String>) -> BundleError {
    BundleError::InvalidSource(message.into())
}

impl InstallJobs {
    /// Start one installation per drive. The source is never modified.
    pub fn start(&self, source: InstallSource, root: PathBuf) -> Result<String, BundleError> {
        let paths = DrivePaths::open(&root)?;
        let root = paths.root().to_path_buf();
        let journal_dir = paths.ensure_dir(&paths.runtime_state().join("install-jobs"))?;
        let id = EntityId::generate("install")
            .map_err(|error| failure(error.to_string()))?
            .as_str()
            .to_owned();
        let job = InstallJob {
            id: id.clone(),
            drive_root: root.clone(),
            state: JobState::Running,
            step: "queued".into(),
            completed_steps: 0,
            total_steps: InstallStep::ALL.len(),
            report: None,
            error: None,
        };
        let journal = journal_dir.join(format!("{id}.json"));
        {
            let mut jobs = self
                .shared
                .jobs
                .lock()
                .map_err(|_| failure("install state unavailable"))?;
            if jobs
                .values()
                .any(|entry| entry.job.drive_root == root && entry.job.state == JobState::Running)
            {
                return Err(failure("an installation is already running on this drive"));
            }
            persist(&journal, &job)?;
            jobs.insert(
                id.clone(),
                Entry {
                    job,
                    cancel: false,
                    journal,
                },
            );
        }
        let manager = self.clone();
        let worker_id = id.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("aidex-install".into())
            .spawn(move || {
                let result = install_with(&source, &root, &mut |step| {
                    manager.advance(&worker_id, step)
                });
                manager.finish(&worker_id, result);
            })
        {
            self.finish(
                &id,
                Err(failure(format!("cannot start installation: {error}"))),
            );
            return Err(failure("cannot start installation worker"));
        }
        Ok(id)
    }

    pub fn status(&self, id: &str) -> Result<InstallJob, BundleError> {
        self.shared
            .jobs
            .lock()
            .map_err(|_| failure("install state unavailable"))?
            .get(id)
            .map(|entry| entry.job.clone())
            .ok_or_else(|| failure("unknown installation job"))
    }

    /// Cancellation takes effect at the next safe step, before activation. Once
    /// pointer switching begins the transaction finishes instead of reporting a
    /// cancelled install whose new app is already active.
    pub fn cancel(&self, id: &str) -> Result<bool, BundleError> {
        let mut jobs = self
            .shared
            .jobs
            .lock()
            .map_err(|_| failure("install state unavailable"))?;
        let entry = jobs
            .get_mut(id)
            .ok_or_else(|| failure("unknown installation job"))?;
        if entry.job.state != JobState::Running
            || matches!(entry.job.step.as_str(), "switch_pointer" | "cleanup")
        {
            return Ok(false);
        }
        entry.cancel = true;
        Ok(true)
    }

    /// Wait without polling, also usable by CLI clients supervising an install.
    pub fn wait(&self, id: &str, timeout: Duration) -> Result<InstallJob, BundleError> {
        let jobs = self
            .shared
            .jobs
            .lock()
            .map_err(|_| failure("install state unavailable"))?;
        if !jobs.contains_key(id) {
            return Err(failure("unknown installation job"));
        }
        let (jobs, _) = self
            .shared
            .changed
            .wait_timeout_while(jobs, timeout, |jobs| {
                jobs.get(id)
                    .is_some_and(|entry| entry.job.state == JobState::Running)
            })
            .map_err(|_| failure("install state unavailable"))?;
        let job = jobs
            .get(id)
            .ok_or_else(|| failure("unknown installation job"))?
            .job
            .clone();
        if job.state == JobState::Running {
            return Err(failure("installation is still running"));
        }
        Ok(job)
    }

    /// Read previous job receipts. An unfinished receipt is interrupted, never
    /// inferred successful from the existence of copied files.
    pub fn history(root: &Path) -> Result<Vec<InstallJob>, BundleError> {
        let paths = DrivePaths::open(root)?;
        let dir = paths.ensure_dir(&paths.runtime_state().join("install-jobs"))?;
        let mut jobs = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|error| failure(error.to_string()))? {
            let path = entry.map_err(|error| failure(error.to_string()))?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| failure(error.to_string()))?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > 1_048_576
            {
                return Err(failure("unsafe installation receipt"));
            }
            let mut job: InstallJob = serde_json::from_slice(
                &fs::read(&path).map_err(|error| failure(error.to_string()))?,
            )
            .map_err(|_| failure("invalid installation receipt"))?;
            if job.state == JobState::Running {
                job.state = JobState::Interrupted;
            }
            jobs.push(job);
        }
        Ok(jobs)
    }

    fn advance(&self, id: &str, step: InstallStep) -> Result<(), BundleError> {
        let mut jobs = self
            .shared
            .jobs
            .lock()
            .map_err(|_| failure("install state unavailable"))?;
        let entry = jobs
            .get_mut(id)
            .ok_or_else(|| failure("unknown installation job"))?;
        if entry.cancel && step != InstallStep::Cleanup {
            return Err(BundleError::Aborted(step));
        }
        entry.job.step = step.as_str().into();
        entry.job.completed_steps = InstallStep::ALL
            .iter()
            .position(|candidate| *candidate == step)
            .unwrap_or(0);
        persist(&entry.journal, &entry.job)?;
        self.shared.changed.notify_all();
        Ok(())
    }

    fn finish(&self, id: &str, result: Result<crate::InstallReport, BundleError>) {
        let Ok(mut jobs) = self.shared.jobs.lock() else {
            return;
        };
        let Some(entry) = jobs.get_mut(id) else {
            return;
        };
        match result {
            Ok(report) => {
                entry.job.state = JobState::Completed;
                entry.job.completed_steps = entry.job.total_steps;
                entry.job.report = serde_json::to_value(report).ok();
                entry.job.step = "completed".into();
            }
            Err(error) => {
                entry.job.state = if matches!(error, BundleError::Aborted(_)) {
                    JobState::Cancelled
                } else {
                    JobState::Failed
                };
                entry.job.error = Some(error.to_string());
            }
        }
        if let Err(error) = persist(&entry.journal, &entry.job) {
            entry.job.state = JobState::Failed;
            entry.job.error = Some(error.to_string());
        }
        self.shared.changed.notify_all();
    }
}

fn persist(path: &Path, job: &InstallJob) -> Result<(), BundleError> {
    use std::io::Write;
    let temp = path.with_extension("tmp");
    let bytes = serde_json::to_vec(job).map_err(|error| failure(error.to_string()))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| failure(error.to_string()))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| failure(error.to_string()))?;
    drop(file);
    fs::rename(&temp, path).map_err(|error| failure(error.to_string()))
}
