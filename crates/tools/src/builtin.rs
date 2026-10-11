//! The built-in tool set.

use schemars::JsonSchema;

use crate::{
    RiskProfile, Tool, ToolDescriptor, ToolError, ToolId,
    fs::{FsDelete, FsGlob, FsList, FsPatch, FsRead, FsSearch, FsWrite},
    git::{GitBranch, GitCommit, GitDiff, GitLog, GitPush, GitShow, GitStatus},
    shell::ShellExec,
};

/// Ids of every built-in tool, in registry order.
pub const BUILTIN_TOOL_IDS: [&str; 15] = [
    "fs.delete",
    "fs.glob",
    "fs.list",
    "fs.patch",
    "fs.read",
    "fs.search",
    "fs.write",
    "git.branch",
    "git.commit",
    "git.diff",
    "git.log",
    "git.push",
    "git.show",
    "git.status",
    "shell.exec",
];

pub(crate) fn descriptor<I: JsonSchema, O: JsonSchema>(
    id: &str,
    version: u32,
    description: &str,
    risk: RiskProfile,
    capabilities: &[&str],
) -> Result<ToolDescriptor, ToolError> {
    Ok(ToolDescriptor::new::<I, O>(
        ToolId::new(id)?,
        version,
        description,
        risk,
        capabilities,
    ))
}

fn boxed<T: Tool + 'static>(tool: Result<T, ToolError>) -> Option<Box<dyn Tool>> {
    tool.ok().map(|tool| Box::new(tool) as Box<dyn Tool>)
}

/// Every built-in tool. Built-in descriptors are constant, so construction only
/// fails if a descriptor is malformed (covered by tests).
pub(crate) fn tools() -> Vec<Box<dyn Tool>> {
    [
        boxed(FsRead::new()),
        boxed(FsList::new()),
        boxed(FsGlob::new()),
        boxed(FsSearch::new()),
        boxed(FsWrite::new()),
        boxed(FsPatch::new()),
        boxed(FsDelete::new()),
        boxed(GitStatus::new()),
        boxed(GitDiff::new()),
        boxed(GitLog::new()),
        boxed(GitShow::new()),
        boxed(GitBranch::new()),
        boxed(GitCommit::new()),
        boxed(GitPush::new()),
        boxed(ShellExec::new()),
    ]
    .into_iter()
    .flatten()
    .collect()
}
