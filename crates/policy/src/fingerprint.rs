//! Canonical SHA-256 fingerprints that bind approvals to one exact intent.

use sha2::{Digest, Sha256};

use crate::{Intent, NetworkTarget};

/// Domain separator; bump the version when the encoding changes.
const DOMAIN: &[u8] = b"aidex.policy.intent.v1\0";

impl Intent {
    /// SHA-256 over a canonical, length-prefixed encoding of every field and attribute.
    ///
    /// Any change — action, ids, summary, a path, the command line, the destination,
    /// the secret, locality, or a flag — changes the fingerprint, so an approval
    /// recorded for one intent cannot be replayed for an altered one. The encoding
    /// is independent of serde field order and of `Debug` output.
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut canon = Canon(Sha256::new());
        canon.bytes(DOMAIN);
        canon.str(self.action.as_str());
        canon.str(self.session_id.as_str());
        for id in [
            &self.workspace_id,
            &self.agent_id,
            &self.tool_id,
            &self.skill_id,
        ] {
            canon.option(id.as_ref().map(|id| id.as_str()));
        }
        canon.str(&self.summary);

        let attributes = &self.attributes;
        canon.count(attributes.paths.len());
        for path in &attributes.paths {
            canon.str(&path.path);
            canon.option(path.workspace_relative.as_deref());
            canon.str(path.class.as_str());
        }
        match &attributes.command {
            None => canon.tag(0),
            Some(command) => {
                canon.tag(1);
                canon.str(command.line());
                canon.str(command.platform().as_str());
                let classification = command.classification();
                canon.str(classification.risk.as_str());
                canon.count(classification.implied_actions.len());
                for action in &classification.implied_actions {
                    canon.str(action.as_str());
                }
                canon.count(classification.network_targets.len());
                for target in &classification.network_targets {
                    canon.target(target);
                }
            }
        }
        match &attributes.network {
            None => canon.tag(0),
            Some(target) => {
                canon.tag(1);
                canon.target(target);
            }
        }
        match &attributes.secret {
            None => canon.tag(0),
            Some(secret) => {
                canon.tag(1);
                canon.str(&secret.name);
                canon.str(&secret.consumer);
            }
        }
        canon.option(attributes.model_locality.map(|locality| locality.as_str()));
        canon.tag(u8::from(attributes.remote_write));
        canon.tag(u8::from(attributes.destructive));
        canon.0.finalize().into()
    }

    /// Lowercase hex encoding of [`Intent::fingerprint`].
    pub fn fingerprint_hex(&self) -> String {
        self.fingerprint()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

struct Canon(Sha256);

impl Canon {
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn tag(&mut self, tag: u8) {
        self.0.update([tag]);
    }

    fn count(&mut self, count: usize) {
        self.0.update((count as u64).to_le_bytes());
    }

    fn str(&mut self, value: &str) {
        self.count(value.len());
        self.0.update(value.as_bytes());
    }

    fn option(&mut self, value: Option<&str>) {
        match value {
            None => self.tag(0),
            Some(value) => {
                self.tag(1);
                self.str(value);
            }
        }
    }

    fn target(&mut self, target: &NetworkTarget) {
        self.str(target.host());
        self.0.update(target.port().to_le_bytes());
        self.str(target.kind().as_str());
    }
}
