//! The flattened view of one evaluation unit that rules and floors inspect.

use crate::{
    Action, Intent, ModelLocality, NetworkTarget, PathScope, SecretUse, ShellClassification,
};

/// Destination knowledge for one unit.
#[derive(Clone, Copy, Debug)]
pub(crate) enum NetworkFact<'a> {
    /// The unit has no network effect.
    Absent,
    /// The unit contacts the network but the destination is unknown.
    Unknown,
    Target(&'a NetworkTarget),
}

/// One unit of evaluation: the intent's action (once per path), or an action the
/// intent's command implies (once per destination).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Facts<'a> {
    pub action: Action,
    pub intent: &'a Intent,
    pub path: Option<&'a PathScope>,
    pub network: NetworkFact<'a>,
}

impl<'a> Facts<'a> {
    pub fn command(&self) -> Option<&'a ShellClassification> {
        self.intent
            .attributes
            .command
            .as_ref()
            .map(|command| command.classification())
    }

    pub fn secret(&self) -> Option<&'a SecretUse> {
        self.intent.attributes.secret.as_ref()
    }

    pub fn locality(&self) -> Option<ModelLocality> {
        self.intent.attributes.model_locality
    }

    pub fn remote_write(&self) -> bool {
        self.intent.attributes.remote_write
    }

    pub fn destructive(&self) -> bool {
        self.intent.attributes.destructive
    }
}
