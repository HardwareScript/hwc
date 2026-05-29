//! Main Design Rule Checker entry point.

use crate::constraint_manager::ConstraintRulebook;

use super::parallel::validate_physics_parallel;
use super::types::{DrcReport, MaterialProperties, NetVoxels};

/// Design Rule Checker: Main entry point for DRC validation.
///
/// Orchestrates the complete DRC validation process and generates
/// beautiful error messages with miette diagnostics.
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 800-1000, DRC validation)
pub struct DesignRuleChecker {
    /// Material properties for thermal calculations
    material: MaterialProperties,
}

impl DesignRuleChecker {
    /// Create a new design rule checker.
    ///
    /// # Arguments
    /// * `material` - Material properties for thermal calculations
    ///
    /// # Examples
    /// ```
    /// use hwc_engine::design_rule_check::{DesignRuleChecker, MaterialProperties};
    ///
    /// let material = MaterialProperties::default();
    /// let checker = DesignRuleChecker::new(material);
    /// ```
    pub fn new(material: MaterialProperties) -> Self {
        Self { material }
    }

    /// Check design rules for a routed board.
    ///
    /// Runs all DRC validators in parallel and generates a detailed report.
    ///
    /// # Arguments
    /// * `nets` - All routed nets with their voxel locations
    /// * `constraints` - Constraint rulebook with all requirements
    /// * `voxel_size_nm` - Size of one voxel in nanometers
    ///
    /// # Returns
    /// DRC report with all violations, warnings, and info
    ///
    /// # Examples
    /// ```
    /// use hwc_engine::design_rule_check::{DesignRuleChecker, NetVoxels, MaterialProperties};
    /// use hwc_engine::{Point3D, constraint_manager::ConstraintRulebook};
    ///
    /// let material = MaterialProperties::default();
    /// let checker = DesignRuleChecker::new(material);
    ///
    /// let nets = vec![
    ///     NetVoxels {
    ///         net_name: "VCC".into(),
    ///         voxels: vec![Point3D::new(0, 0, 0)],
    ///     },
    /// ];
    ///
    /// let constraints = ConstraintRulebook::new(500_000);
    /// let report = checker.check(&nets, &constraints, 500_000);
    ///
    /// if !report.is_valid() {
    ///     println!("{}", report);
    /// }
    /// ```
    pub fn check(
        &self,
        nets: &[NetVoxels],
        constraints: &ConstraintRulebook,
        voxel_size_nm: i64,
    ) -> DrcReport {
        // Run parallel validation
        validate_physics_parallel(nets, constraints, &self.material, voxel_size_nm)
    }
}

impl Default for DesignRuleChecker {
    fn default() -> Self {
        Self::new(MaterialProperties::default())
    }
}
