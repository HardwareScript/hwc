//! Parallel validation orchestration using Rayon.

use crate::constraint_manager::ConstraintRulebook;
use crate::space::HardwareSpace;

use super::clearance::validate_clearances;
use super::crosstalk::validate_crosstalk; // v0.3.0: Signal integrity
use super::electromigration::validate_electromigration; // P21
use super::junction::validate_junction_breakdown; // P46
use super::thermal::validate_thermal_rise; // P22
use super::trace_width::validate_trace_widths;
use super::types::DrcReport;
use super::via_checks::{
    validate_drill_to_drill_clearance, validate_layer_specific_via_enclosure,
    validate_via_diameters_analytic, validate_via_enclosure_analytic,
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

    // 3. Electromigration Validation (P21: Current density limits)
    let em_violations =
        validate_electromigration(&space.analytic_routes, &space.material_registry)?;
    for violation in em_violations {
        report.add_violation(violation);
    }

    // 4. Thermal Rise Validation (P22: I²R self-heating)
    let max_temp_rise_c = space
        .fabrication_constraints
        .as_ref()
        .and_then(|c| c.thermal.as_ref())
        .map(|t| t.max_temp_rise_c)
        .ok_or_else(|| {
            "[DRC THERMAL] FATAL: Profile must declare thermal constraints with max_temp_rise. \
             Add 'thermal:' section to your profile with max_temp_rise field."
                .to_string()
        })?;
    
    let thermal_violations = validate_thermal_rise(
        &space.analytic_routes,
        &space.material_registry,
        max_temp_rise_c,
    )?;
    for violation in thermal_violations {
        report.add_violation(violation);
    }

    // 5. Via Diameter Validation
    let via_diameter_report = validate_via_diameters_analytic(&space.contacts, constraints)?;
    for violation in via_diameter_report.violations {
        report.add_violation(violation);
    }

    // 6. Via Enclosure Validation
    let via_enclosure_report =
        validate_via_enclosure_analytic(&space.contacts, constraints, space.technology_strategy)?;
    for violation in via_enclosure_report.violations {
        report.add_violation(violation);
    }

    // 7. Drill-to-Drill Clearance Validation
    let drill_clearance_report = validate_drill_to_drill_clearance(&space.contacts, constraints)?;
    for violation in drill_clearance_report.violations {
        report.add_violation(violation);
    }

    // 8. Layer-Specific Via Enclosure Validation (v0.2.2: ASIC device landing rules)
    let layer_enclosure_report = validate_layer_specific_via_enclosure(space, constraints)?;
    for violation in layer_enclosure_report.violations {
        report.add_violation(violation);
    }
    for warning in layer_enclosure_report.warnings {
        report.add_warning(warning);
    }

    // 9. Crosstalk Validation (v0.3.0: Signal integrity)
    let crosstalk_violations = validate_crosstalk(space, constraints)?;
    for violation in crosstalk_violations {
        report.add_violation(violation);
    }

    // 10. Junction Breakdown Validation (P46: Semiconductor junction voltage rating)
    let junction_violations = validate_junction_breakdown(space, &space.material_registry)?;
    for violation in junction_violations {
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
