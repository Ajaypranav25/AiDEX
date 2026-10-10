//! Tool errors with stable service codes and machine-readable reasons.

use std::{error::Error, fmt, io};

use aidex_core::ServiceErrorCode;
use aidex_events::EventError;
use aidex_sandbox::SandboxError;
use aidex_workspace::WorkspaceError;

/// Why an input argument was rejected. The wire form (`as_str`) is stable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum InputError {
    /// The input is not a JSON object.
    NotAnObject,
    /// A field the schema does not declare (schemas are closed).
    UnknownField,
    /// A required field is absent.
    MissingField,
    /// A field has the wrong JSON type.
    InvalidType,
    /// A field has the right type but an unacceptable value.
    InvalidValue,
    /// A path is malformed, absolute, or uses traversal or platform-reserved names.
    InvalidPath,
    /// A glob or regular expression does not compile.
    InvalidPattern,
    /// A number or length is outside the accepted range.
    OutOfRange,
    /// Fields that cannot be combined, or one field requires another.
    ConflictingFields,
}

impl InputError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotAnObject => "not_an_object",
            Self::UnknownField => "unknown_field",
            Self::MissingField => "missing_field",
            Self::InvalidType => "invalid_type",
            Self::InvalidValue => "invalid_value",
            Self::InvalidPath => "invalid_path",
            Self::InvalidPattern => "invalid_pattern",
            Self::OutOfRange => "out_of_range",
            Self::ConflictingFields => "conflicting_fields",
        }
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A process launcher refused or failed to run a tool's process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchError {
    /// The launcher (broker) does not recognise or no longer honours the permit.
    Denied(String),
    /// No launcher capable of this request is available.
    Unavailable(String),
    /// The program could not be resolved to an executable.
    ProgramNotFound(String),
    /// The sandbox could not enforce or launch the request (fail closed).
    Sandbox(SandboxError),
    /// The operating system refused to start the process.
    Spawn(String),
    /// Any other failure while running or observing the process.
    Io(String),
}

impl LaunchError {
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::Denied(_) => ServiceErrorCode::PermissionDenied,
            Self::Unavailable(_) => ServiceErrorCode::Unavailable,
            Self::ProgramNotFound(_) => ServiceErrorCode::NotFound,
            Self::Sandbox(error) => error.code(),
            Self::Spawn(_) | Self::Io(_) => ServiceErrorCode::Internal,
        }
    }
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied(reason) => write!(f, "the process launch was denied: {reason}"),
            Self::Unavailable(reason) => write!(f, "no process launcher is available: {reason}"),
            Self::ProgramNotFound(program) => write!(f, "program `{program}` was not found"),
            Self::Sandbox(error) => write!(f, "sandbox: {error}"),
            Self::Spawn(reason) => write!(f, "the process could not be started: {reason}"),
            Self::Io(reason) => write!(f, "process I/O failed: {reason}"),
        }
    }
}

impl Error for LaunchError {}

/// Every failure a tool or the registry reports. Messages name the caller's own
/// inputs and workspace-relative paths; they never include file contents.
#[derive(Debug)]
pub enum ToolError {
    /// No tool with this id (or model-facing name) is registered.
    UnknownTool(String),
    /// A tool could not be registered (duplicate id or model name, bad descriptor).
    Registration(String),
    /// The input failed schema validation. `field` is a path such as `edits[1].old`
    /// (empty for the whole input).
    InvalidInput {
        field: String,
        reason: InputError,
        message: String,
    },
    /// The path resolves outside the workspace (traversal, symbolic link, junction).
    EscapesRoot { path: String },
    /// A create/replace target is itself a symbolic link or junction.
    SymlinkLeaf { path: String },
    /// The path does not exist.
    NotFound { path: String },
    /// The entry exists but is not the expected kind.
    WrongKind {
        path: String,
        expected: &'static str,
        found: &'static str,
    },
    /// Tools may not modify this path (for example anything inside `.git`).
    Forbidden { path: String, reason: String },
    /// A configured size or count limit was exceeded.
    TooLarge { what: String, size: u64, limit: u64 },
    /// The target changed after it was planned (or concurrently); nothing was written.
    Conflict { path: String, reason: String },
    /// An edit or hunk of a patch did not apply; nothing was written.
    PatchFailed { index: usize, reason: String },
    /// The plan does not match the permit, was altered after planning, or belongs to
    /// a different tool, workspace, session, or call.
    PlanMismatch(String),
    /// The permit cannot authorize this plan (for example a read-only permit for a
    /// writing tool).
    PermitRejected(String),
    /// This plan was already executed; plans are single-use.
    AlreadyExecuted,
    /// The call was cancelled.
    Cancelled,
    /// The tool runs a process and the context has no [`crate::ProcessLauncher`].
    LauncherUnavailable,
    /// The process launcher refused or failed.
    Launch(LaunchError),
    /// A process the tool depends on exited unsuccessfully.
    ProcessFailed {
        program: String,
        exit_code: Option<i32>,
        stderr: String,
    },
    /// A process exceeded its wall-clock timeout and was terminated.
    TimedOut { program: String, after_ms: u64 },
    /// The workspace root is not the root of a git repository.
    NotARepository,
    /// A workspace failure not covered above (root unavailable, metadata errors).
    Workspace(WorkspaceError),
    /// The `tool.started` event could not be recorded, so nothing was executed.
    Events(EventError),
    /// The tool produced output that does not match its output contract.
    InvalidOutput(String),
    /// An operating-system I/O failure, with the operation that failed.
    Io { context: String, source: io::Error },
}

impl ToolError {
    /// Stable service category for API mapping.
    pub fn code(&self) -> ServiceErrorCode {
        match self {
            Self::InvalidInput { .. } | Self::WrongKind { .. } | Self::PatchFailed { .. } => {
                ServiceErrorCode::InvalidArgument
            }
            Self::UnknownTool(_) | Self::NotFound { .. } | Self::NotARepository => {
                ServiceErrorCode::NotFound
            }
            Self::EscapesRoot { .. }
            | Self::SymlinkLeaf { .. }
            | Self::Forbidden { .. }
            | Self::PermitRejected(_) => ServiceErrorCode::PermissionDenied,
            Self::TooLarge { .. } => ServiceErrorCode::InvalidArgument,
            Self::Conflict { .. } | Self::PlanMismatch(_) | Self::AlreadyExecuted => {
                ServiceErrorCode::Conflict
            }
            Self::Cancelled | Self::LauncherUnavailable | Self::TimedOut { .. } => {
                ServiceErrorCode::Unavailable
            }
            Self::Launch(error) => error.code(),
            Self::Workspace(error) => error.code(),
            Self::Events(error) => error.code(),
            Self::Registration(_)
            | Self::ProcessFailed { .. }
            | Self::InvalidOutput(_)
            | Self::Io { .. } => ServiceErrorCode::Internal,
        }
    }

    /// Stable snake_case reason, finer-grained than [`Self::code`]; used as the
    /// `code` of `tool.failed` events.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::UnknownTool(_) => "unknown_tool",
            Self::Registration(_) => "registration",
            Self::InvalidInput { .. } => "invalid_input",
            Self::EscapesRoot { .. } => "escapes_root",
            Self::SymlinkLeaf { .. } => "symlink_leaf",
            Self::NotFound { .. } => "not_found",
            Self::WrongKind { .. } => "wrong_kind",
            Self::Forbidden { .. } => "forbidden_path",
            Self::TooLarge { .. } => "too_large",
            Self::Conflict { .. } => "conflict",
            Self::PatchFailed { .. } => "patch_failed",
            Self::PlanMismatch(_) => "plan_mismatch",
            Self::PermitRejected(_) => "permit_rejected",
            Self::AlreadyExecuted => "already_executed",
            Self::Cancelled => "cancelled",
            Self::LauncherUnavailable => "launcher_unavailable",
            Self::Launch(_) => "launch_failed",
            Self::ProcessFailed { .. } => "process_failed",
            Self::TimedOut { .. } => "timed_out",
            Self::NotARepository => "not_a_repository",
            Self::Workspace(error) => error.reason(),
            Self::Events(_) => "events",
            Self::InvalidOutput(_) => "invalid_output",
            Self::Io { .. } => "io",
        }
    }

    pub(crate) fn invalid(
        field: impl Into<String>,
        reason: InputError,
        message: impl Into<String>,
    ) -> Self {
        Self::InvalidInput {
            field: field.into(),
            reason,
            message: message.into(),
        }
    }

    pub(crate) fn io(context: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub(crate) fn conflict(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Conflict {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Map a workspace resolution failure for the input `field`.
    pub(crate) fn from_workspace(field: &str, error: WorkspaceError) -> Self {
        match error {
            WorkspaceError::InvalidPath { path, reason } => Self::invalid(
                field,
                InputError::InvalidPath,
                format!("invalid path `{path}`: {reason}"),
            ),
            WorkspaceError::EscapesRoot(path) => Self::EscapesRoot { path },
            WorkspaceError::SymlinkLeaf(path) => Self::SymlinkLeaf {
                path: path.as_str().to_owned(),
            },
            WorkspaceError::PathNotFound(path) => Self::NotFound {
                path: path.as_str().to_owned(),
            },
            other => Self::Workspace(other),
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool(id) => write!(f, "no tool `{id}` is registered"),
            Self::Registration(reason) => write!(f, "cannot register tool: {reason}"),
            Self::InvalidInput {
                field,
                reason,
                message,
            } => {
                if field.is_empty() {
                    write!(f, "invalid input ({reason}): {message}")
                } else {
                    write!(f, "invalid input `{field}` ({reason}): {message}")
                }
            }
            Self::EscapesRoot { path } => write!(
                f,
                "`{path}` resolves outside the workspace (traversal, symbolic link, or junction)"
            ),
            Self::SymlinkLeaf { path } => write!(
                f,
                "refusing to replace `{path}`: it is a symbolic link or junction"
            ),
            Self::NotFound { path } => write!(f, "`{path}` does not exist in the workspace"),
            Self::WrongKind {
                path,
                expected,
                found,
            } => write!(f, "`{path}` is a {found}, not a {expected}"),
            Self::Forbidden { path, reason } => write!(f, "`{path}` cannot be modified: {reason}"),
            Self::TooLarge { what, size, limit } => {
                write!(f, "{what} is too large ({size} bytes; limit {limit})")
            }
            Self::Conflict { path, reason } => {
                write!(f, "`{path}` changed since it was planned: {reason}")
            }
            Self::PatchFailed { index, reason } => {
                write!(f, "patch step {index} does not apply: {reason}")
            }
            Self::PlanMismatch(reason) => write!(f, "plan rejected: {reason}"),
            Self::PermitRejected(reason) => write!(f, "permit rejected: {reason}"),
            Self::AlreadyExecuted => f.write_str("this plan was already executed"),
            Self::Cancelled => f.write_str("the tool call was cancelled"),
            Self::LauncherUnavailable => {
                f.write_str("this tool runs a process but no process launcher is configured")
            }
            Self::Launch(error) => error.fmt(f),
            Self::ProcessFailed {
                program,
                exit_code,
                stderr,
            } => {
                match exit_code {
                    Some(code) => write!(f, "`{program}` exited with status {code}")?,
                    None => write!(f, "`{program}` was terminated")?,
                }
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
            Self::TimedOut { program, after_ms } => {
                write!(f, "`{program}` timed out after {after_ms} ms")
            }
            Self::NotARepository => {
                f.write_str("the workspace root is not the root of a git repository")
            }
            Self::Workspace(error) => error.fmt(f),
            Self::Events(error) => write!(f, "the tool event could not be recorded: {error}"),
            Self::InvalidOutput(reason) => write!(f, "the tool produced invalid output: {reason}"),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl Error for ToolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Launch(error) => Some(error),
            Self::Workspace(error) => Some(error),
            Self::Events(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<LaunchError> for ToolError {
    fn from(error: LaunchError) -> Self {
        Self::Launch(error)
    }
}
