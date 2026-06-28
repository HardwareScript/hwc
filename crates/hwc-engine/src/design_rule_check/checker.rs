//! Main Design Rule Checker entry point.

use crate::constraint_manager::ConstraintRulebook;

use crate::space::HardwareSpace;

use super::parallel::validate_physics_parallel;
use super::types::DrcReport;

/// Design Rule Checker: Main entry point for DRC validation.
///
/// Orchestrates the complete DRC validation process and generates
/// beautiful error messages with miette diagnostics.
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 800-1000, DRC validation)
pub struct DesignRuleChecker;

impl DesignRuleChecker {
    /// Create a new design rule checker.
    pub fn new() -> Self {
        Self
    }

    /// Check design rules for a routed space.
    ///
    /// Runs all DRC validators in parallel and generates a detailed report.
    pub fn check(
        &self,
        space: &HardwareSpace,
        constraints: &ConstraintRulebook,
    ) -> Result<DrcReport, String> {
        // Run parallel validation
        validate_physics_parallel(space, constraints)
    }
}

impl Default for DesignRuleChecker {
    fn default() -> Self {
        Self::new()
    }
}
