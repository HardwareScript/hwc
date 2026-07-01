//! Parallel validation orchestration using Rayon.

use crate::constraint_manager::ConstraintRulebook;
use crate::space::HardwareSpace;

use super::clearance::validate_clearances;
use super::trace_width::validate_trace_widths;
use super::thermal::validate_current_density;
use super::types::DrcReport;
use super::via_checks::{
    validate_drill_to_drill_clearance, validate_via_diameters_analytic,
    validate_via_enclosure_analytic,
};

/// Validate physics in parallel using Rayon.
pub fn validate_physics_parallel(
    space: &HardwareSpace,
    constraints: &ConstraintRulebook,
) -> Result<DrcReport, String> {
    let mut report = DrcReport::new();

    // 1. Clearance Validation (analytic)
    let clearance_violations = validate_clearances(space, constraints)?;
    for violation in clearance_violations {
        report.add_violation(violation);
    }

    // 2. Trace Width Validation (analytic)
    let width_violations = validate_trace_widths(space, constraints)?;
    for violation in width_violations {
        report.add_violation(violation);
    }

    // 3. Current Density Validation (PDK material limits)
    let current_density_violations = validate_current_density(
        &space.analytic_routes,
        &space.material_registry,
    )?;
    for violation in current_density_violations {
        report.add_violation(violation);
    }

    // 4. Via Diameter Validation
    let via_diameter_report = validate_via_diameters_analytic(
        &space.contacts,
        constraints,
    )?;
    for violation in via_diameter_report.violations {
        report.add_violation(violation);
    }

    // 5. Via Enclosure Validation
    let via_enclosure_report = validate_via_enclosure_analytic(
        &space.contacts,
        constraints,
    )?;
    for violation in via_enclosure_report.violations {
        report.add_violation(violation);
    }

    // 6. Drill-to-Drill Clearance Validation
    let drill_clearance_report = validate_drill_to_drill_clearance(
        &space.contacts,
        constraints,
    )?;
    for violation in drill_clearance_report.violations {
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
