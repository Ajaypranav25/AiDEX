//! On-disk TOML policy documents (closed schema, `schema_version = 1`).

mod project;
mod set;

use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    time::Duration,
};

use aidex_core::EntityId;
use serde::Deserialize;

pub use project::{PROJECT_RULES_FILE, ProjectInfo, ProjectRules};
pub use set::PolicySet;

use crate::{
    ActionPattern, ApprovalScope, Conditions, DomainPattern, Layer, ModelLocality, NetworkAccess,
    NetworkKind, NetworkSettings, PathClass, PathGlobs, PathLocation, PolicyError, Rule,
    SandboxLevel, ShellRisk, Verdict, decision::DEFAULT_APPROVAL_EXPIRY,
};

/// The only policy document schema this build reads.
pub const SCHEMA_VERSION: i64 = 1;
/// Policy files larger than this are rejected.
pub const MAX_DOCUMENT_BYTES: u64 = 256 * 1024;

/// A parsed, validated policy document for one layer and subject.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDocument {
    /// Where the document came from (file name or caller label).
    pub source: String,
    pub layer: Layer,
    /// Host, workspace, agent, skill, tool, or session id; `None` for `global`.
    pub subject: Option<EntityId>,
    /// Network settings; only `global` documents may define them.
    pub network: Option<NetworkSettings>,
    pub rules: Vec<Rule>,
}

impl PolicyDocument {
    /// Parse and validate a document. Malformed TOML, unknown fields, unknown
    /// actions or values, unsupported schema versions, and inconsistent rules are
    /// errors: a policy file is never partially applied or silently ignored.
    pub fn parse(text: &str, source: impl Into<String>) -> Result<Self, PolicyError> {
        let source = source.into();
        check_schema(text, &source)?;
        let file: DocumentFile = from_toml(text, &source)?;
        let subject = match (file.scope.requires_subject(), file.subject) {
            (true, Some(subject)) => Some(EntityId::new(subject).map_err(|error| {
                PolicyError::invalid(&source, format!("invalid subject: {error}"))
            })?),
            (true, None) => {
                return Err(PolicyError::invalid(
                    &source,
                    format!("scope `{}` requires a `subject` id", file.scope),
                ));
            }
            (false, Some(_)) => {
                return Err(PolicyError::invalid(
                    &source,
                    "scope `global` must not have a `subject`",
                ));
            }
            (false, None) => None,
        };
        if file.network.is_some() && file.scope != Layer::Global {
            return Err(PolicyError::invalid(
                &source,
                "only `global` documents may define `[network]`",
            ));
        }
        let network = file.network.map(|network| {
            let defaults = NetworkSettings::default();
            NetworkSettings {
                allow_domains: network.allow_domains,
                local_network: network.local_network.unwrap_or(defaults.local_network),
                arbitrary_ip: network.arbitrary_ip.unwrap_or(defaults.arbitrary_ip),
            }
        });
        let rules = file
            .rules
            .into_iter()
            .map(|rule| rule.into_rule(&source))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            source,
            layer: file.scope,
            subject,
            network,
            rules,
        })
    }
}

/// Read the version first so a newer schema is reported as such, not as unknown fields.
pub(crate) fn check_schema(text: &str, source: &str) -> Result<(), PolicyError> {
    #[derive(Deserialize)]
    struct Probe {
        schema_version: Option<i64>,
    }
    let probe: Probe = from_toml(text, source)?;
    match probe.schema_version {
        Some(SCHEMA_VERSION) => Ok(()),
        Some(found) => Err(PolicyError::UnsupportedSchema {
            source: source.to_owned(),
            found,
        }),
        None => Err(PolicyError::Parse {
            source: source.to_owned(),
            message: "missing `schema_version`".to_owned(),
        }),
    }
}

pub(crate) fn from_toml<T: for<'de> Deserialize<'de>>(
    text: &str,
    source: &str,
) -> Result<T, PolicyError> {
    toml::from_str(text).map_err(|error| {
        let line = error
            .span()
            .map(|span| text[..span.start.min(text.len())].lines().count().max(1));
        let message = error.message().trim().to_owned();
        PolicyError::Parse {
            source: source.to_owned(),
            message: match line {
                Some(line) => format!("line {line}: {message}"),
                None => message,
            },
        }
    })
}

/// Read a regular, non-symlinked file of at most [`MAX_DOCUMENT_BYTES`].
pub(crate) fn read_policy_file(path: &Path) -> Result<String, PolicyError> {
    let io_error = |error: std::io::Error| PolicyError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    };
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(PolicyError::UnsafePath(path.to_path_buf()));
    }
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(PolicyError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_DOCUMENT_BYTES,
        });
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io_error)?
        .take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(PolicyError::TooLarge {
            path: path.to_path_buf(),
            limit: MAX_DOCUMENT_BYTES,
        });
    }
    String::from_utf8(bytes).map_err(|_| PolicyError::Parse {
        source: path.display().to_string(),
        message: "file is not valid UTF-8".to_owned(),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentFile {
    #[serde(rename = "schema_version")]
    _schema_version: i64,
    scope: Layer,
    subject: Option<String>,
    network: Option<NetworkFile>,
    #[serde(default)]
    rules: Vec<RuleFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NetworkFile {
    #[serde(default)]
    allow_domains: Vec<DomainPattern>,
    local_network: Option<NetworkAccess>,
    arbitrary_ip: Option<NetworkAccess>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum VerdictKind {
    Allow,
    Deny,
    Sandbox,
    RequireApproval,
    RateLimit,
}

impl VerdictKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Sandbox => "sandbox",
            Self::RequireApproval => "require_approval",
            Self::RateLimit => "rate_limit",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    id: String,
    action: ActionPattern,
    verdict: VerdictKind,
    reason: Option<String>,
    sandbox: Option<SandboxLevel>,
    approval_scopes: Option<Vec<ApprovalScope>>,
    expires_in_secs: Option<u64>,
    rate_max: Option<u32>,
    rate_per_secs: Option<u64>,
    #[serde(default)]
    when: WhenFile,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WhenFile {
    location: Option<PathLocation>,
    #[serde(default)]
    paths: Vec<String>,
    path_class: Option<PathClass>,
    min_risk: Option<ShellRisk>,
    max_risk: Option<ShellRisk>,
    #[serde(default)]
    only_programs: Vec<String>,
    #[serde(default)]
    any_programs: Vec<String>,
    #[serde(default)]
    hosts: Vec<DomainPattern>,
    #[serde(default)]
    network_kinds: Vec<NetworkKind>,
    model_locality: Option<ModelLocality>,
    remote_write: Option<bool>,
    destructive: Option<bool>,
    #[serde(default)]
    secret_names: Vec<String>,
    #[serde(default)]
    consumers: Vec<String>,
}

impl RuleFile {
    fn into_rule(self, source: &str) -> Result<Rule, PolicyError> {
        let id = self.id;
        let unexpected = |field: &str| {
            PolicyError::invalid(
                source,
                format!(
                    "rule `{id}`: `{field}` is not valid for verdict `{}`",
                    self.verdict.as_str()
                ),
            )
        };
        let approval_fields = self.approval_scopes.is_some() || self.expires_in_secs.is_some();
        let rate_fields = self.rate_max.is_some() || self.rate_per_secs.is_some();
        if self.verdict != VerdictKind::Sandbox && self.sandbox.is_some() {
            return Err(unexpected("sandbox"));
        }
        if self.verdict != VerdictKind::RequireApproval && approval_fields {
            return Err(unexpected("approval_scopes/expires_in_secs"));
        }
        if self.verdict != VerdictKind::RateLimit && rate_fields {
            return Err(unexpected("rate_max/rate_per_secs"));
        }
        let verdict = match self.verdict {
            VerdictKind::Allow => Verdict::Allow,
            VerdictKind::Deny => Verdict::Deny,
            VerdictKind::Sandbox => match self.sandbox {
                Some(SandboxLevel::Direct) => {
                    return Err(PolicyError::invalid(
                        source,
                        format!("rule `{id}`: `direct` is not a sandbox level a rule may require"),
                    ));
                }
                Some(level) => Verdict::Sandbox { level },
                None => {
                    return Err(PolicyError::invalid(
                        source,
                        format!("rule `{id}`: verdict `sandbox` requires `sandbox = \"<level>\"`"),
                    ));
                }
            },
            VerdictKind::RequireApproval => Verdict::RequireApproval {
                scope_options: self
                    .approval_scopes
                    .unwrap_or_else(|| vec![ApprovalScope::Once, ApprovalScope::Session]),
                expires_in: self
                    .expires_in_secs
                    .map_or(DEFAULT_APPROVAL_EXPIRY, Duration::from_secs),
            },
            VerdictKind::RateLimit => match (self.rate_max, self.rate_per_secs) {
                (Some(max), Some(per)) => Verdict::RateLimit {
                    max,
                    per: Duration::from_secs(per),
                },
                _ => {
                    return Err(PolicyError::invalid(
                        source,
                        format!(
                            "rule `{id}`: verdict `rate_limit` requires `rate_max` and `rate_per_secs`"
                        ),
                    ));
                }
            },
        };
        let when = self.when;
        let paths = if when.paths.is_empty() {
            None
        } else {
            Some(PathGlobs::new(&when.paths)?)
        };
        let rule = Rule {
            id,
            action: self.action,
            when: Conditions {
                location: when.location,
                paths,
                path_class: when.path_class,
                min_risk: when.min_risk,
                max_risk: when.max_risk,
                only_programs: when.only_programs,
                any_programs: when.any_programs,
                hosts: when.hosts,
                network_kinds: when.network_kinds,
                model_locality: when.model_locality,
                remote_write: when.remote_write,
                destructive: when.destructive,
                secret_names: when.secret_names,
                consumers: when.consumers,
            },
            verdict,
            reason: self.reason,
        };
        rule.validate()
            .map_err(|message| PolicyError::invalid(source, message))?;
        Ok(rule)
    }
}
