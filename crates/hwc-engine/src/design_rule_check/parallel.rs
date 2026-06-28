//! Parallel validation orchestration using Rayon.

use crate::constraint_manager::ConstraintRulebook;
use crate::space::HardwareSpace;

use super::clearance::validate_clearances;
use super::thermal::validate_thermal_analytic;
use super::trace_width::validate_trace_widths;
use super::types::DrcReport;

/// Validate physics in parallel using Rayon.
pub fn validate_physics_parallel(
    space: &HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Result<DrcReport, String> {
    let mut report = DrcReport::new();

    // 1. Clearance Validation (now analytic)
    let clearance_violations = validate_clearances(space, constraints);
    for violation in clearance_violations {
        report.add_violation(violation);
    }

    // 2. Trace Width Validation (now analytic)
    let width_violations = validate_trace_widths(space, constraints);
    for violation in width_violations {
        report.add_violation(violation);
    }

    // 3. Thermal Validation (analytic)
    let thermal_violations = validate_thermal_analytic(
        &space.analytic_routes,
        constraints,
        &space.material_registry,
    )?;
    for violation in thermal_violations {
        report.add_violation(violation);
    }

    // Add summary info
    if report.is_valid() {
        report.add_info("All DRC checks passed".into());
    } else {
        report.add_info(
            format!(
                "Found {} violation(s) that must be fixed",
                report.violations.len()
            )
            .into(),
        );
    }

    Ok(report)
}

/// Validate physics in single-threaded mode (for comparison/testing).
pub fn validate_physics_sequential(
    space: &HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Result<DrcReport, String> {
    validate_physics_parallel(space, constraints)
}
