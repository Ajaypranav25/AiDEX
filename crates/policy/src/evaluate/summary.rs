//! Human-readable effect summaries for approval cards and audit records.

use crate::{Intent, ModelLocality, PathClass};

/// Paths listed before the rest are counted.
const MAX_LISTED_PATHS: usize = 3;

/// Describe what `intent` would do. Command lines are redacted; secret values never
/// appear (intents only carry secret names).
pub(super) fn effect_summary(intent: &Intent) -> String {
    let attributes = &intent.attributes;
    let mut text = intent.action.label().to_owned();
    if !intent.summary.trim().is_empty() {
        text.push_str(": ");
        text.push_str(intent.summary.trim());
    }
    let mut facts = Vec::new();
    if !attributes.paths.is_empty() {
        let mut listed: Vec<String> = attributes
            .paths
            .iter()
            .take(MAX_LISTED_PATHS)
            .map(|path| {
                let mut notes = Vec::new();
                if !path.inside_workspace() {
                    notes.push("outside the workspace");
                }
                match path.class {
                    PathClass::Ordinary => {}
                    PathClass::Sensitive => notes.push("sensitive"),
                    PathClass::Protected => notes.push("AiDEX-protected"),
                }
                if notes.is_empty() {
                    path.path.clone()
                } else {
                    format!("{} ({})", path.path, notes.join(", "))
                }
            })
            .collect();
        let hidden = attributes.paths.len().saturating_sub(MAX_LISTED_PATHS);
        if hidden > 0 {
            listed.push(format!("{hidden} more"));
        }
        facts.push(format!("Paths: {}", listed.join("; ")));
    }
    if let Some(command) = &attributes.command {
        let classification = command.classification();
        let mut line = format!(
            "Command: `{}` (risk: {}",
            command.redacted_line(),
            classification.risk
        );
        if !classification.implied_actions.is_empty() {
            let implied: Vec<&str> = classification
                .implied_actions
                .iter()
                .map(|action| action.as_str())
                .collect();
            line.push_str("; also ");
            line.push_str(&implied.join(", "));
        }
        line.push(')');
        facts.push(line);
    }
    if let Some(target) = &attributes.network {
        facts.push(format!("Destination: {target} ({})", target.kind()));
    }
    if let Some(secret) = &attributes.secret {
        facts.push(format!(
            "Secret: `{}` for `{}`",
            secret.name, secret.consumer
        ));
    }
    if let Some(locality) = attributes.model_locality {
        facts.push(
            match locality {
                ModelLocality::Local => "Model: local",
                ModelLocality::Remote => "Model: remote provider",
            }
            .to_owned(),
        );
    }
    if attributes.remote_write {
        facts.push("Writes to a remote system".to_owned());
    }
    if attributes.destructive {
        facts.push("Irreversible".to_owned());
    }
    for fact in facts {
        text.push_str(". ");
        text.push_str(&fact);
    }
    text.push('.');
    text
}
