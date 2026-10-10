//! Execution permits: the broker's authorization to execute one sealed plan.

use aidex_core::EntityId;
use serde::Serialize;

use crate::{PlanFingerprint, ToolDescriptor, ToolError, ToolPlan};

/// Who or what authorized a plan; recorded for audit and checked against the tool.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PermitIssuer {
    /// Policy allowed the plan's intents outright (the deciding rule id).
    Policy { rule: String },
    /// A human approved the plan.
    Approval { approval_id: EntityId },
    /// The broker auto-permitted a read-only tool. Refused for any tool whose
    /// descriptor is not read-only.
    ReadOnly,
    /// Development and tests only; a production broker never issues it.
    Development,
}

/// Authorization to execute exactly the plan with [`Self::fingerprint`].
///
/// The constructor is public because the broker is the issuer; the type carries no
/// secret. What prevents unbrokered execution is ownership: only the broker holds
/// the [`crate::ToolRegistry`], and the registry consumes the permit, refuses any
/// plan whose fingerprint differs or whose content was altered, and executes each
/// plan at most once. The harness holds a broker handle, never the registry.
#[derive(Debug, Eq, PartialEq)]
pub struct ExecutionPermit {
    fingerprint: PlanFingerprint,
    issuer: PermitIssuer,
}

impl ExecutionPermit {
    pub fn new(fingerprint: PlanFingerprint, issuer: PermitIssuer) -> Self {
        Self {
            fingerprint,
            issuer,
        }
    }

    /// A permit for `plan`'s current fingerprint.
    pub fn for_plan(plan: &ToolPlan, issuer: PermitIssuer) -> Self {
        Self::new(plan.fingerprint(), issuer)
    }

    pub fn fingerprint(&self) -> PlanFingerprint {
        self.fingerprint
    }

    pub fn issuer(&self) -> &PermitIssuer {
        &self.issuer
    }

    /// Check that this permit authorizes `plan` for the tool `descriptor`: the
    /// fingerprints match, the plan content still hashes to its fingerprint, the
    /// plan belongs to this tool and version, and a read-only permit is only used
    /// for a read-only tool.
    pub fn authorize(&self, plan: &ToolPlan, descriptor: &ToolDescriptor) -> Result<(), ToolError> {
        if self.fingerprint != plan.fingerprint() {
            return Err(ToolError::PlanMismatch(
                "the permit was issued for a different plan".into(),
            ));
        }
        plan.verify_integrity()?;
        if plan.tool_id() != &descriptor.id || plan.tool_version() != descriptor.version {
            return Err(ToolError::PlanMismatch(format!(
                "the plan was made by `{}` v{}, not `{}` v{}",
                plan.tool_id(),
                plan.tool_version(),
                descriptor.id,
                descriptor.version
            )));
        }
        if self.issuer == PermitIssuer::ReadOnly && !descriptor.risk.is_read_only() {
            return Err(ToolError::PermitRejected(format!(
                "`{}` is not read-only and needs a policy or approval permit",
                descriptor.id
            )));
        }
        Ok(())
    }
}
