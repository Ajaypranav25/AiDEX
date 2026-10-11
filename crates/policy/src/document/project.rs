//! Workspace project rules (spec §55): tightening-only guardrails declared by a project.

use std::path::Path;

use serde::Deserialize;

use super::{check_schema, from_toml, read_policy_file};
use crate::{ActionPattern, PathGlobs, PolicyError};

/// Recommended location of project rules, relative to a workspace root.
pub const PROJECT_RULES_FILE: &str = ".aidex/rules.toml";

/// Informational project metadata. It is not policy and grants nothing.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectInfo {
    pub package_manager: Option<String>,
    pub tests: Option<String>,
    pub lint: Option<String>,
    pub build: Option<String>,
}

/// Rules a workspace declares about itself.
///
/// Project files live in repository content, which is untrusted, so they can only
/// tighten policy: `never_modify` denies writes and deletes beneath the listed
/// paths, and `require_approval` makes the listed actions require approval. Both
/// apply as floors that no layer, profile, or session rule can loosen.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectRules {
    pub source: String,
    pub project: ProjectInfo,
    never_modify: Option<PathGlobs>,
    require_approval: Vec<ActionPattern>,
}

impl ProjectRules {
    pub fn parse(text: &str, source: impl Into<String>) -> Result<Self, PolicyError> {
        let source = source.into();
        check_schema(text, &source)?;
        let file: ProjectFile = from_toml(text, &source)?;
        let project = file.project.unwrap_or_default();
        let agent = file.agent.unwrap_or_default();
        let never_modify = if agent.never_modify.is_empty() {
            None
        } else {
            Some(PathGlobs::new(&agent.never_modify)?)
        };
        Ok(Self {
            source,
            project: ProjectInfo {
                package_manager: project.package_manager,
                tests: project.tests,
                lint: project.lint,
                build: project.build,
            },
            never_modify,
            require_approval: agent.require_approval,
        })
    }

    /// Read project rules from a regular, non-symlinked file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, PolicyError> {
        let path = path.as_ref();
        let text = read_policy_file(path)?;
        Self::parse(&text, path.display().to_string())
    }

    /// Workspace-relative patterns that must never be written or deleted.
    pub fn never_modify(&self) -> &[String] {
        self.never_modify
            .as_ref()
            .map_or(&[], |globs| globs.patterns())
    }

    /// Actions that always require approval in this workspace.
    pub fn require_approval(&self) -> &[ActionPattern] {
        &self.require_approval
    }

    pub(crate) fn protects(&self, relative: &str) -> bool {
        self.never_modify
            .as_ref()
            .is_some_and(|globs| globs.matches(relative))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    #[serde(rename = "schema_version")]
    _schema_version: i64,
    project: Option<ProjectSection>,
    agent: Option<AgentSection>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectSection {
    package_manager: Option<String>,
    tests: Option<String>,
    lint: Option<String>,
    build: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentSection {
    #[serde(default)]
    never_modify: Vec<String>,
    #[serde(default)]
    require_approval: Vec<ActionPattern>,
}
