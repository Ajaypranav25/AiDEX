//! The loaded set of layer rules, network settings, and project rules.

use std::{collections::BTreeMap, fs, io, path::Path};

use aidex_core::EntityId;

use super::{PolicyDocument, ProjectRules, read_policy_file};
use crate::{EvaluationContext, Intent, Layer, NetworkSettings, PolicyError, Rule};

/// A rule together with the document it came from.
#[derive(Clone, Debug)]
pub(crate) struct LoadedRule {
    pub rule: Rule,
    pub source: Option<String>,
}

/// All policy inputs other than the per-session [`EvaluationContext`].
#[derive(Clone, Debug, Default)]
pub struct PolicySet {
    layers: BTreeMap<(Layer, Option<EntityId>), Vec<LoadedRule>>,
    network: NetworkSettings,
    network_source: Option<String>,
    projects: BTreeMap<EntityId, ProjectRules>,
}

impl PolicySet {
    /// An empty set: only built-in profiles, floors, and hard rules apply.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load every `*.toml` file directly inside `dir` (normally
    /// `DrivePaths::policies()`), in file-name order.
    ///
    /// Fails closed: a missing directory, a symlinked directory or file, an
    /// oversized file, or any malformed document is an error and nothing is loaded.
    /// Other files and subdirectories are ignored.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, PolicyError> {
        let dir = dir.as_ref();
        let io_error = |error: io::Error| PolicyError::Io {
            path: dir.to_path_buf(),
            message: error.to_string(),
        };
        match fs::symlink_metadata(dir) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(PolicyError::UnsafePath(dir.to_path_buf()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(PolicyError::MissingDirectory(dir.to_path_buf()));
            }
            Err(error) => return Err(io_error(error)),
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(dir).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let path = entry.path();
            let is_toml = path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"));
            if is_toml {
                files.push((entry.file_name().to_string_lossy().into_owned(), path));
            }
        }
        files.sort();
        let mut set = Self::new();
        for (name, path) in files {
            let text = read_policy_file(&path)?;
            set.add_document(PolicyDocument::parse(&text, name)?)?;
        }
        Ok(set)
    }

    pub fn from_documents(
        documents: impl IntoIterator<Item = PolicyDocument>,
    ) -> Result<Self, PolicyError> {
        let mut set = Self::new();
        for document in documents {
            set.add_document(document)?;
        }
        Ok(set)
    }

    /// Merge a document. Rule ids must be unique per layer and subject, and only
    /// one document may define network settings.
    pub fn add_document(&mut self, document: PolicyDocument) -> Result<(), PolicyError> {
        if let Some(network) = document.network {
            if let Some(previous) = &self.network_source {
                return Err(PolicyError::invalid(
                    &document.source,
                    format!("network settings are already defined by `{previous}`"),
                ));
            }
            self.network = network;
            self.network_source = Some(document.source.clone());
        }
        for rule in document.rules {
            self.insert(
                document.layer,
                document.subject.clone(),
                rule,
                Some(document.source.clone()),
            )?;
        }
        Ok(())
    }

    /// Add one rule programmatically. `subject` must be `None` exactly for `Global`.
    pub fn add_rule(
        &mut self,
        layer: Layer,
        subject: Option<EntityId>,
        rule: Rule,
    ) -> Result<(), PolicyError> {
        if layer.requires_subject() != subject.is_some() {
            return Err(PolicyError::invalid(
                &format!("{layer} layer"),
                if layer.requires_subject() {
                    "this layer requires a subject id"
                } else {
                    "the global layer has no subject"
                },
            ));
        }
        rule.validate()
            .map_err(|message| PolicyError::invalid(&format!("{layer} layer"), message))?;
        self.insert(layer, subject, rule, None)
    }

    fn insert(
        &mut self,
        layer: Layer,
        subject: Option<EntityId>,
        rule: Rule,
        source: Option<String>,
    ) -> Result<(), PolicyError> {
        let label = match &subject {
            Some(subject) => format!("{layer} ({subject})"),
            None => layer.to_string(),
        };
        let rules = self.layers.entry((layer, subject)).or_default();
        if rules.iter().any(|loaded| loaded.rule.id == rule.id) {
            return Err(PolicyError::DuplicateRule {
                layer: label,
                id: rule.id,
            });
        }
        rules.push(LoadedRule { rule, source });
        Ok(())
    }

    /// Attach a workspace's project rules (replacing earlier ones).
    pub fn set_project_rules(&mut self, workspace: EntityId, rules: ProjectRules) {
        self.projects.insert(workspace, rules);
    }

    pub fn project_rules(&self, workspace: &EntityId) -> Option<&ProjectRules> {
        self.projects.get(workspace)
    }

    pub fn network(&self) -> &NetworkSettings {
        &self.network
    }

    /// Rules defined for one layer and subject.
    pub fn rules(&self, layer: Layer, subject: Option<&EntityId>) -> impl Iterator<Item = &Rule> {
        self.layers
            .get(&(layer, subject.cloned()))
            .into_iter()
            .flatten()
            .map(|loaded| &loaded.rule)
    }

    /// Number of layer rules across all layers and subjects.
    pub fn rule_count(&self) -> usize {
        self.layers.values().map(Vec::len).sum()
    }

    /// Rule lists that apply to `intent`, least specific layer first.
    pub(crate) fn applicable<'a>(
        &'a self,
        context: &EvaluationContext,
        intent: &Intent,
    ) -> Vec<(Layer, &'a [LoadedRule])> {
        let subjects = [
            (Layer::Global, None),
            (Layer::Machine, context.host_id.clone()),
            (Layer::Workspace, intent.workspace_id.clone()),
            (Layer::Agent, intent.agent_id.clone()),
            (Layer::Skill, intent.skill_id.clone()),
            (Layer::Tool, intent.tool_id.clone()),
            (Layer::Session, Some(intent.session_id.clone())),
        ];
        subjects
            .into_iter()
            .filter(|(layer, subject)| layer.requires_subject() == subject.is_some())
            .filter_map(|key| {
                let layer = key.0;
                self.layers.get(&key).map(|rules| (layer, rules.as_slice()))
            })
            .collect()
    }
}
