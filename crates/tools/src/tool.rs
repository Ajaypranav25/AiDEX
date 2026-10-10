//! The two-phase tool contract.

use serde_json::Value;

use crate::{ExecutionPermit, ToolContext, ToolDescriptor, ToolError, ToolOutput, ToolPlan};

/// A typed tool with a side-effect-free `plan` phase and a permit-gated `execute`.
///
/// - `plan` validates the input against the typed schema (stable
///   [`ToolError::InvalidInput`] errors), resolves every path through the
///   context's `ConfinedRoot`, computes [`crate::PlanFacts`] for policy, and for
///   writes precomputes the reviewable diff. It must not modify anything or spawn
///   processes.
/// - `execute` runs exactly the sealed plan, re-verifying its targets (a changed
///   target is a [`ToolError::Conflict`], never a clobber).
///
/// Implementations are only ever invoked through [`crate::ToolRegistry`], which
/// verifies the permit, the plan's integrity and context, single use, and write
/// locks before calling `execute`, and emits the lifecycle events. The registry
/// never hands registered tools back out.
pub trait Tool: Send + Sync {
    fn descriptor(&self) -> &ToolDescriptor;

    fn plan(&self, ctx: &ToolContext, input: Value) -> Result<ToolPlan, ToolError>;

    fn execute(
        &self,
        ctx: &ToolContext,
        plan: &ToolPlan,
        permit: &ExecutionPermit,
    ) -> Result<ToolOutput, ToolError>;

    /// Workspace-relative paths the registry locks for the duration of `execute`.
    /// Defaults to the plan's write paths; a directory key also excludes writers
    /// beneath it.
    fn write_locks(&self, plan: &ToolPlan) -> Vec<String> {
        plan.facts()
            .write_paths()
            .map(|path| path.path.clone())
            .collect()
    }
}
