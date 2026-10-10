//! Project rules (spec §55) and project instruction files.
//!
//! `.aidex/rules.toml` is parsed by [`aidex_policy::ProjectRules`] (closed schema,
//! `schema_version = 1`): an informational `[project]` table and `[agent]`
//! guardrails. Repository content is untrusted, so the guardrails can only tighten
//! policy: they attach to a [`PolicySet`] as project floors (`never_modify` denies
//! writes and deletes, `require_approval` forces approval) that no layer, profile,
//! or session rule can loosen, and the schema has no way to allow anything.
//!
//! `AGENTS.md`, `CLAUDE.md`, and `.aidex/instructions.md` at the workspace root are
//! surfaced as project guidance: shown to the model as project-trust data, never
//! as user instructions.

use std::{fs, io::Read};

use aidex_core::{EntityId, WorkspacePath};
use aidex_policy::{ActionPattern, MAX_DOCUMENT_BYTES, PROJECT_RULES_FILE, PolicySet, ProjectInfo};
use aidex_workspace::{Access, ConfinedRoot, WorkspaceError};
use serde::{Deserialize, Serialize};

use crate::{ContextError, exclude, fsio};

/// Instruction files read from the workspace root, in this order.
pub const INSTRUCTION_FILES: [&str; 3] = ["AGENTS.md", "CLAUDE.md", ".aidex/instructions.md"];
/// Longest instruction text kept per file; longer files are cut with a marker.
pub const MAX_INSTRUCTION_BYTES: usize = 64 * 1024;

/// One project instruction file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInstructions {
    pub path: WorkspacePath,
    /// Redacted text, cut at [`MAX_INSTRUCTION_BYTES`].
    pub text: String,
    pub bytes: u64,
    pub truncated: bool,
    /// Credentials were redacted from the text.
    pub redacted: bool,
    /// SHA-256 of the file as read.
    pub sha256: String,
}

/// A workspace's rules file and instruction files.
#[derive(Clone, Debug, Default)]
pub struct ProjectRules {
    rules: Option<aidex_policy::ProjectRules>,
    instructions: Vec<ProjectInstructions>,
}

/// Serializable view of [`ProjectRules`] (for `rules.show`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesView {
    /// `.aidex/rules.toml` when the workspace has one.
    pub source: Option<String>,
    pub project: ProjectCommands,
    pub never_modify: Vec<String>,
    pub require_approval: Vec<String>,
    pub instructions: Vec<ProjectInstructions>,
}

/// Informational project commands. They grant nothing.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCommands {
    pub package_manager: Option<String>,
    pub tests: Option<String>,
    pub lint: Option<String>,
    pub build: Option<String>,
}

impl ProjectRules {
    /// Load `.aidex/rules.toml` and the instruction files from a workspace root.
    ///
    /// Missing files are fine. A rules file that is invalid, too large, symlinked,
    /// or resolves outside the root is an error (fail closed: the guardrails it
    /// declares must not be silently dropped). Unreadable instruction files are
    /// skipped; secret-looking ones are never read.
    pub fn load(root: &ConfinedRoot) -> Result<Self, ContextError> {
        let rules = match read_confined(root, PROJECT_RULES_FILE, MAX_DOCUMENT_BYTES as u64)? {
            None => None,
            Some(file) => {
                let text = String::from_utf8(file.bytes).map_err(|_| {
                    ContextError::Rules(format!("{PROJECT_RULES_FILE} is not UTF-8"))
                })?;
                Some(
                    aidex_policy::ProjectRules::parse(&text, PROJECT_RULES_FILE)
                        .map_err(|error| ContextError::Rules(error.to_string()))?,
                )
            }
        };
        let mut instructions = Vec::new();
        for name in INSTRUCTION_FILES {
            if exclude::is_secret_path(name) {
                continue;
            }
            let file = match read_confined(root, name, MAX_INSTRUCTION_BYTES as u64) {
                Ok(Some(file)) => file,
                Ok(None) | Err(_) => continue,
            };
            let text = String::from_utf8_lossy(&file.bytes);
            if exclude::contains_private_key(&text) {
                continue;
            }
            let redacted = exclude::redact(&text);
            let was_redacted = matches!(redacted, std::borrow::Cow::Owned(_));
            let path = WorkspacePath::new(name)
                .map_err(|error| ContextError::InvalidRequest(error.to_string()))?;
            instructions.push(ProjectInstructions {
                path,
                text: redacted.into_owned(),
                bytes: file.size,
                truncated: file.truncated,
                redacted: was_redacted,
                sha256: fsio::sha256_hex(&file.bytes),
            });
        }
        Ok(Self {
            rules,
            instructions,
        })
    }

    /// The parsed rules file, if the workspace has one.
    pub fn policy_rules(&self) -> Option<&aidex_policy::ProjectRules> {
        self.rules.as_ref()
    }

    /// The rules as an `aidex-policy` project layer, to attach with
    /// [`PolicySet::set_project_rules`]. `None` when there is no rules file.
    pub fn to_policy_layer(&self) -> Option<aidex_policy::ProjectRules> {
        self.rules.clone()
    }

    /// Attach the rules to `policies` for `workspace` as tightening-only floors.
    /// Returns whether anything was attached.
    pub fn apply_to(&self, policies: &mut PolicySet, workspace: &EntityId) -> bool {
        match self.to_policy_layer() {
            Some(rules) => {
                policies.set_project_rules(workspace.clone(), rules);
                true
            }
            None => false,
        }
    }

    pub fn project(&self) -> ProjectCommands {
        let info: Option<&ProjectInfo> = self.rules.as_ref().map(|rules| &rules.project);
        info.map(|info| ProjectCommands {
            package_manager: info.package_manager.clone(),
            tests: info.tests.clone(),
            lint: info.lint.clone(),
            build: info.build.clone(),
        })
        .unwrap_or_default()
    }

    pub fn never_modify(&self) -> &[String] {
        self.rules.as_ref().map_or(&[], |rules| rules.never_modify())
    }

    pub fn require_approval(&self) -> &[ActionPattern] {
        self.rules
            .as_ref()
            .map_or(&[], |rules| rules.require_approval())
    }

    pub fn instructions(&self) -> &[ProjectInstructions] {
        &self.instructions
    }

    /// Whether neither a rules file nor any instruction file exists.
    pub fn is_empty(&self) -> bool {
        self.rules.is_none() && self.instructions.is_empty()
    }

    pub fn view(&self) -> RulesView {
        RulesView {
            source: self.rules.as_ref().map(|_| PROJECT_RULES_FILE.to_owned()),
            project: self.project(),
            never_modify: self.never_modify().to_vec(),
            require_approval: self
                .require_approval()
                .iter()
                .map(ToString::to_string)
                .collect(),
            instructions: self.instructions.clone(),
        }
    }

    /// A compact, human-readable rendering of the rules file for a context pack.
    pub fn summary_text(&self) -> Option<String> {
        self.rules.as_ref()?;
        let project = self.project();
        let mut lines = vec![format!("Project rules from {PROJECT_RULES_FILE}:")];
        for (label, value) in [
            ("package manager", &project.package_manager),
            ("test command", &project.tests),
            ("lint command", &project.lint),
            ("build command", &project.build),
        ] {
            if let Some(value) = value {
                lines.push(format!("- {label}: {value}"));
            }
        }
        if !self.never_modify().is_empty() {
            lines.push(format!(
                "- never modify: {}",
                self.never_modify().join(", ")
            ));
        }
        if !self.require_approval().is_empty() {
            let actions: Vec<String> = self
                .require_approval()
                .iter()
                .map(ToString::to_string)
                .collect();
            lines.push(format!("- requires approval: {}", actions.join(", ")));
        }
        Some(exclude::redact(&lines.join("\n")).into_owned())
    }
}

struct ConfinedFile {
    bytes: Vec<u8>,
    size: u64,
    truncated: bool,
}

/// Read `relative` inside `root`: `None` when missing; links that leave the root,
/// symlinked leaves, and non-files are errors. Files over `max` bytes are an error
/// for the rules file and cut for instructions (the caller decides).
fn read_confined(
    root: &ConfinedRoot,
    relative: &str,
    max: u64,
) -> Result<Option<ConfinedFile>, ContextError> {
    let path = WorkspacePath::new(relative)
        .map_err(|error| ContextError::InvalidRequest(error.to_string()))?;
    let absolute = match root.resolve(&path, Access::Read) {
        Ok(absolute) => absolute,
        Err(WorkspaceError::PathNotFound(_)) => return Ok(None),
        Err(error) => return Err(ContextError::Rules(format!("{relative}: {error}"))),
    };
    let metadata = fs::symlink_metadata(&absolute)
        .map_err(|error| ContextError::io(format!("cannot inspect {relative}"), error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ContextError::Rules(format!(
            "{relative} must be a regular file, not a link or directory"
        )));
    }
    let size = metadata.len();
    let is_rules = relative == PROJECT_RULES_FILE;
    if is_rules && size > max {
        return Err(ContextError::Rules(format!(
            "{relative} is larger than {max} bytes"
        )));
    }
    let mut bytes = Vec::new();
    fs::File::open(&absolute)
        .and_then(|file| file.take(max).read_to_end(&mut bytes))
        .map_err(|error| ContextError::io(format!("cannot read {relative}"), error))?;
    let truncated = size > max;
    if truncated {
        // Cut on a character boundary.
        if let Err(error) = std::str::from_utf8(&bytes) {
            if error.error_len().is_none() {
                bytes.truncate(error.valid_up_to());
            }
        }
    }
    Ok(Some(ConfinedFile {
        bytes,
        size,
        truncated,
    }))
}
