//! Parallel validation orchestration using Rayon.

use crate::constraint_manager::ConstraintRulebook;

use super::clearance::validate_clearances;
use super::thermal::validate_thermal;
use super::trace_width::validate_trace_widths;
use super::types::{DrcReport, MaterialProperties, NetVoxels};

/// Validate physics in parallel using Rayon.
///
/// **TRUE DATA PARALLELISM**: Each validator function internally parallelizes over nets
/// using Rayon's par_iter(). This spreads the massive dataset (10,000+ nets) across
/// all CPU cores, not just 3 cores for 3 validators.
///
/// **Algorithm**:
/// 1. Run all validators (they internally use par_iter over nets)
/// 2. Each validator uses ALL CPU cores to process nets in parallel
/// 3. Collect all violations
/// 4. Aggregate into final report
///
/// **Why This Is Fast**:
/// - Validators parallelize over nets (10,000+ items), not validators (3 items)
/// - All CPU cores are utilized efficiently
/// - O(1) trace length calculations instead of O(N) voxel iteration
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 900-1000, Parallel Validation)
///
/// # Arguments
/// * `nets` - All routed nets with their voxel locations
/// * `constraints` - Constraint rulebook with all requirements
/// * `material` - Material properties for thermal calculations
/// * `voxel_size_nm` - Size of one voxel in nanometers
///
/// # Returns
/// Complete DRC report with all violations, warnings, and info
///
/// # Examples
/// ```
/// use hwc_engine::design_rule_check::{validate_physics_parallel, NetVoxels, MaterialProperties};
/// use hwc_engine::{Point3D, constraint_manager::ConstraintRulebook};
///
/// let nets = vec![
///     NetVoxels {
///         net_name: "VCC".into(),
///         voxels: vec![Point3D::new(0, 0, 0)],
///     },
/// ];
///
/// let constraints = ConstraintRulebook::new(500_000);
/// let material = MaterialProperties::default();
///
/// let report = validate_physics_parallel(&nets, &constraints, &material, 500_000);
/// assert!(report.is_valid());
/// ```
pub fn validate_physics_parallel(
    nets: &[NetVoxels],
    constraints: &ConstraintRulebook,
    material: &MaterialProperties,
    voxel_size_nm: i64,
) -> DrcReport {
    let mut report = DrcReport::new();

    // eprintln!($3"[DEBUG DRC PARALLEL] Starting clearance validation...");
    // Run validators sequentially, but each validator internally parallelizes over nets
    // This is more efficient than parallelizing 3 validators
    let clearance_violations = validate_clearances(nets, constraints);
    // eprintln!($3"[DEBUG DRC PARALLEL] Clearance validation complete: {} violations", clearance_violations.len());
    for violation in clearance_violations {
        report.add_violation(violation);
    }

    // eprintln!($3"[DEBUG DRC PARALLEL] Starting trace width validation...");
    let width_violations = validate_trace_widths(nets, constraints, voxel_size_nm);
    // eprintln!($3"[DEBUG DRC PARALLEL] Trace width validation complete: {} violations", width_violations.len());
    for violation in width_violations {
        report.add_violation(violation);
    }

    // eprintln!($3"[DEBUG DRC PARALLEL] Starting thermal validation...");
    let thermal_violations = validate_thermal(nets, constraints, material, voxel_size_nm);
    // eprintln!($3"[DEBUG DRC PARALLEL] Thermal validation complete: {} violations", thermal_violations.len());
    for violation in thermal_violations {
        report.add_violation(violation);
    }

    // eprintln!($3"[DEBUG DRC PARALLEL] Starting via diameter validation...");
    // Task 4.2: Via checks require ContactMetadata and substrate layers (not available in parallel validator)
    // Via checks are performed separately in validation.rs with full HardwareSpace context
    // eprintln!($3"[DEBUG DRC PARALLEL] Via checks skipped (performed separately with analytic geometry)");

    // eprintln!($3"[DEBUG DRC PARALLEL] All validations complete");

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

    report
}

/// Validate physics in single-threaded mode (for comparison/testing).
///
/// This is the baseline implementation that runs all validators sequentially.
/// Used for performance comparison and determinism verification.
///
/// # Arguments
/// * `nets` - All routed nets with their voxel locations
/// * `constraints` - Constraint rulebook with all requirements
/// * `material` - Material properties for thermal calculations
/// * `voxel_size_nm` - Size of one voxel in nanometers
///
/// # Returns
/// Complete DRC report with all violations, warnings, and info
pub fn validate_physics_sequential(
    nets: &[NetVoxels],
    constraints: &ConstraintRulebook,
    material: &MaterialProperties,
    voxel_size_nm: i64,
) -> DrcReport {
    let mut report = DrcReport::new();

    // Run validators sequentially
    let clearance_violations = validate_clearances(nets, constraints);
    for violation in clearance_violations {
        report.add_violation(violation);
    }

    let width_violations = validate_trace_widths(nets, constraints, voxel_size_nm);
    for violation in width_violations {
        report.add_violation(violation);
    }

    let thermal_violations = validate_thermal(nets, constraints, material, voxel_size_nm);
    for violation in thermal_violations {
        report.add_violation(violation);
    }

    // Task 4.2: Via checks require ContactMetadata and substrate layers (not available in sequential validator)
    // Via checks are performed separately in validation.rs with full HardwareSpace context
    // eprintln!($3"[DEBUG DRC SEQUENTIAL] Via checks skipped (performed separately with analytic geometry)");

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

    report
}
