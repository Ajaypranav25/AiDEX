//! Shared fixtures for aidex-tools integration tests.
#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use aidex_core::EntityId;
use aidex_events::{EventSink, MemoryEventSink};
use aidex_tools::{
    DirectLauncher, ExecutionPermit, PermitIssuer, ToolContext, ToolError, ToolId, ToolOutput,
    ToolPlan, ToolRegistry,
};
use aidex_workspace::ConfinedRoot;
use serde_json::Value;

/// A temporary workspace with a registry, an in-memory event sink, and stable ids.
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub root: Arc<ConfinedRoot>,
    pub events: Arc<MemoryEventSink>,
    pub registry: ToolRegistry,
    pub session: EntityId,
    pub workspace: EntityId,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("workspace dir");
        let root = Arc::new(ConfinedRoot::new(dir.path()).expect("confined root"));
        Self {
            dir,
            root,
            events: Arc::new(MemoryEventSink::new()),
            registry: ToolRegistry::with_builtin_tools(),
            session: EntityId::generate("ses").unwrap(),
            workspace: EntityId::generate("wsp").unwrap(),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// A fresh call context with the direct (test-only) launcher.
    pub fn ctx(&self) -> ToolContext {
        let events: Arc<dyn EventSink> = self.events.clone();
        ToolContext::new(
            self.session.clone(),
            self.workspace.clone(),
            EntityId::generate("call").unwrap(),
            self.root.clone(),
            events,
        )
        .with_launcher(Arc::new(DirectLauncher::new()))
    }

    pub fn write(&self, relative: &str, content: impl AsRef<[u8]>) {
        let path = self.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path().join(relative)).unwrap()
    }

    pub fn plan_in(
        &self,
        ctx: &ToolContext,
        tool: &str,
        input: Value,
    ) -> Result<ToolPlan, ToolError> {
        self.registry.plan(ctx, &ToolId::new(tool).unwrap(), input)
    }

    pub fn plan(&self, tool: &str, input: Value) -> Result<(ToolContext, ToolPlan), ToolError> {
        let ctx = self.ctx();
        let plan = self.plan_in(&ctx, tool, input)?;
        Ok((ctx, plan))
    }

    pub fn execute(&self, ctx: &ToolContext, plan: &ToolPlan) -> Result<ToolOutput, ToolError> {
        self.registry.execute(
            ctx,
            plan,
            ExecutionPermit::for_plan(plan, PermitIssuer::Development),
        )
    }

    /// Plan and execute in one step.
    pub fn run(&self, tool: &str, input: Value) -> Result<ToolOutput, ToolError> {
        let (ctx, plan) = self.plan(tool, input)?;
        self.execute(&ctx, &plan)
    }
}

/// A directory link: a symbolic link on Unix, a junction on Windows.
pub fn dir_link(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).expect("create symlink");
        true
    }
    #[cfg(windows)]
    {
        let created = Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !created {
            eprintln!("note: could not create a junction; skipping link assertions");
        }
        created
    }
}

/// A file symbolic link; on Windows this needs Developer Mode or elevation.
pub fn file_link(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).expect("create symlink");
        true
    }
    #[cfg(windows)]
    {
        match std::os::windows::fs::symlink_file(target, link) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("note: file symlinks are not permitted here ({error}); skipping");
                false
            }
        }
    }
}

/// Whether `git` runs on this host.
pub fn git_available() -> bool {
    let available = Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        eprintln!("note: git is not available; skipping git assertions");
    }
    available
}

/// Run git in `dir` for test setup.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Initialize a repository with a local identity and one commit of `files`.
pub fn init_repo(dir: &Path, files: &[(&str, &str)]) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "AiDEX Test"]);
    git(dir, &["config", "user.email", "test@example.invalid"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    for (path, content) in files {
        let full = dir.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

pub fn outside_dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("secret.txt"), "outside").unwrap();
    let path = dir.path().to_path_buf();
    (dir, path)
}
