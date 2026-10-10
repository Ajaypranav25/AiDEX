//! Policy layers, rules, match conditions, and workspace path globs.

use std::fmt;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use crate::{
    ActionPattern, DomainPattern, ModelLocality, NetworkKind, PathClass, PolicyError, ShellRisk,
    Verdict,
    facts::{Facts, NetworkFact},
    macros::string_enum,
};

/// Longest approval expiry a rule may request.
const MAX_EXPIRY_SECS: u64 = 24 * 60 * 60;
/// Longest rule id.
const MAX_RULE_ID: usize = 64;

string_enum! {
    /// Where a rule is defined, least specific first (spec §13).
    pub enum Layer("policy layer") {
        Global => "global",
        Machine => "machine",
        Workspace => "workspace",
        Agent => "agent",
        Skill => "skill",
        Tool => "tool",
        Session => "session",
    }
}

impl Layer {
    /// Every layer except `Global` applies to one subject (host, workspace, ...).
    pub const fn requires_subject(self) -> bool {
        !matches!(self, Self::Global)
    }
}

string_enum! {
    /// Whether a path condition targets the workspace or outside it.
    pub enum PathLocation("path location") {
        Workspace => "workspace",
        Outside => "outside",
    }
}

/// A conditional verdict for matching actions.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// Unique within its layer and subject: 1-64 of `a-z 0-9 . _ -`.
    pub id: String,
    pub action: ActionPattern,
    pub when: Conditions,
    pub verdict: Verdict,
    pub reason: Option<String>,
}

impl Rule {
    pub fn new(id: impl Into<String>, action: impl Into<ActionPattern>, verdict: Verdict) -> Self {
        Self {
            id: id.into(),
            action: action.into(),
            when: Conditions::default(),
            verdict,
            reason: None,
        }
    }

    pub fn when(mut self, when: Conditions) -> Self {
        self.when = when;
        self
    }

    pub fn because(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Check the id, verdict parameters, and condition ranges.
    pub fn validate(&self) -> Result<(), String> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= MAX_RULE_ID
            && self.id.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            });
        if !id_ok {
            return Err(format!(
                "rule id `{}` must be 1-{MAX_RULE_ID} characters of a-z, 0-9, `.`, `_`, `-`",
                crate::error::truncate(&self.id)
            ));
        }
        match &self.verdict {
            Verdict::RequireApproval {
                scope_options,
                expires_in,
            } => {
                if scope_options.is_empty() {
                    return Err(format!("rule `{}` lists no approval scopes", self.id));
                }
                if expires_in.as_secs() == 0 || expires_in.as_secs() > MAX_EXPIRY_SECS {
                    return Err(format!(
                        "rule `{}` approval expiry must be 1-{MAX_EXPIRY_SECS} seconds",
                        self.id
                    ));
                }
            }
            Verdict::RateLimit { max, per } if *max == 0 || per.as_secs() == 0 => {
                return Err(format!(
                    "rule `{}` rate limit needs max >= 1 and a period of at least one second",
                    self.id
                ));
            }
            _ => {}
        }
        if let (Some(min), Some(max)) = (self.when.min_risk, self.when.max_risk) {
            if min > max {
                return Err(format!("rule `{}` has min_risk above max_risk", self.id));
            }
        }
        Ok(())
    }

    pub(crate) fn matches(&self, facts: &Facts<'_>) -> bool {
        self.action.matches(facts.action) && self.when.matches(facts)
    }
}

/// Conditions that must all hold for a rule to match. A condition about a fact the
/// unit does not have (a path condition on an intent without paths) does not match.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conditions {
    pub location: Option<PathLocation>,
    /// Workspace-relative globs; outside paths never match.
    pub paths: Option<PathGlobs>,
    pub path_class: Option<PathClass>,
    pub min_risk: Option<ShellRisk>,
    pub max_risk: Option<ShellRisk>,
    /// Every program the command runs is one of these (safe for allow rules).
    pub only_programs: Vec<String>,
    /// At least one program the command runs is one of these (for deny rules).
    pub any_programs: Vec<String>,
    pub hosts: Vec<DomainPattern>,
    pub network_kinds: Vec<NetworkKind>,
    pub model_locality: Option<ModelLocality>,
    pub remote_write: Option<bool>,
    pub destructive: Option<bool>,
    pub secret_names: Vec<String>,
    pub consumers: Vec<String>,
}

impl Conditions {
    fn matches(&self, facts: &Facts<'_>) -> bool {
        self.path_matches(facts)
            && self.command_matches(facts)
            && self.network_matches(facts)
            && self
                .model_locality
                .is_none_or(|locality| facts.locality() == Some(locality))
            && self
                .remote_write
                .is_none_or(|flag| facts.remote_write() == flag)
            && self
                .destructive
                .is_none_or(|flag| facts.destructive() == flag)
            && self.secret_matches(facts)
    }

    fn path_matches(&self, facts: &Facts<'_>) -> bool {
        if self.location.is_none() && self.paths.is_none() && self.path_class.is_none() {
            return true;
        }
        let Some(path) = facts.path else {
            return false;
        };
        let location_ok = self.location.is_none_or(|location| {
            (location == PathLocation::Workspace) == path.inside_workspace()
        });
        let glob_ok = self.paths.as_ref().is_none_or(|globs| {
            path.workspace_relative
                .as_deref()
                .is_some_and(|relative| globs.matches(relative))
        });
        location_ok && glob_ok && self.path_class.is_none_or(|class| path.class == class)
    }

    fn command_matches(&self, facts: &Facts<'_>) -> bool {
        if self.min_risk.is_none()
            && self.max_risk.is_none()
            && self.only_programs.is_empty()
            && self.any_programs.is_empty()
        {
            return true;
        }
        let Some(command) = facts.command() else {
            return false;
        };
        let listed = |names: &[String], program: &String| {
            names.iter().any(|name| name.eq_ignore_ascii_case(program))
        };
        self.min_risk.is_none_or(|min| command.risk >= min)
            && self.max_risk.is_none_or(|max| command.risk <= max)
            && (self.only_programs.is_empty()
                || (!command.programs.is_empty()
                    && command
                        .programs
                        .iter()
                        .all(|program| listed(&self.only_programs, program))))
            && (self.any_programs.is_empty()
                || command
                    .programs
                    .iter()
                    .any(|program| listed(&self.any_programs, program)))
    }

    fn network_matches(&self, facts: &Facts<'_>) -> bool {
        if self.hosts.is_empty() && self.network_kinds.is_empty() {
            return true;
        }
        let NetworkFact::Target(target) = facts.network else {
            return false;
        };
        (self.hosts.is_empty() || self.hosts.iter().any(|host| host.matches(target.host())))
            && (self.network_kinds.is_empty() || self.network_kinds.contains(&target.kind()))
    }

    fn secret_matches(&self, facts: &Facts<'_>) -> bool {
        if self.secret_names.is_empty() && self.consumers.is_empty() {
            return true;
        }
        let Some(secret) = facts.secret() else {
            return false;
        };
        (self.secret_names.is_empty() || self.secret_names.contains(&secret.name))
            && (self.consumers.is_empty() || self.consumers.contains(&secret.consumer))
    }
}

/// Workspace-relative path globs (`*` stays within a component, `**` spans
/// components, matching is ASCII case-insensitive). A pattern also matches
/// everything beneath it, so `migrations/production` protects the directory tree.
#[derive(Clone)]
pub struct PathGlobs {
    patterns: Vec<String>,
    set: GlobSet,
}

impl PathGlobs {
    pub fn new<I, S>(patterns: I) -> Result<Self, PolicyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut builder = GlobSetBuilder::new();
        let mut kept = Vec::new();
        for pattern in patterns {
            let pattern = pattern.as_ref();
            let trimmed = validate_glob(pattern)?;
            for candidate in [trimmed.to_owned(), format!("{trimmed}/**")] {
                let glob = GlobBuilder::new(&candidate)
                    .literal_separator(true)
                    .case_insensitive(true)
                    .backslash_escape(true)
                    .build()
                    .map_err(|error| glob_error(pattern, error.kind().to_string()))?;
                builder.add(glob);
            }
            kept.push(pattern.to_owned());
        }
        if kept.is_empty() {
            return Err(glob_error("", "at least one pattern is required"));
        }
        let set = builder
            .build()
            .map_err(|error| glob_error(&kept.join(", "), error.to_string()))?;
        Ok(Self {
            patterns: kept,
            set,
        })
    }

    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    /// Match a `/`-separated workspace-relative path.
    pub fn matches(&self, relative: &str) -> bool {
        self.set.is_match(relative)
    }
}

impl fmt::Debug for PathGlobs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PathGlobs").field(&self.patterns).finish()
    }
}

impl PartialEq for PathGlobs {
    fn eq(&self, other: &Self) -> bool {
        self.patterns == other.patterns
    }
}

fn validate_glob(pattern: &str) -> Result<&str, PolicyError> {
    let trimmed = pattern.trim().trim_end_matches('/');
    let reason = if trimmed.is_empty() {
        Some("pattern is empty")
    } else if trimmed.starts_with('/') || trimmed.as_bytes().get(1) == Some(&b':') {
        Some("pattern must be workspace-relative")
    } else if trimmed.contains('\\') {
        Some("use `/` as the path separator")
    } else if trimmed.split('/').any(|part| part == "..") {
        Some("pattern must not contain `..`")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(glob_error(pattern, reason)),
        None => Ok(trimmed),
    }
}

fn glob_error(pattern: &str, reason: impl Into<String>) -> PolicyError {
    PolicyError::InvalidGlob {
        pattern: crate::error::truncate(pattern),
        reason: reason.into(),
    }
}
