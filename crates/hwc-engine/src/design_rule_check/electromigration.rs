//! Static Electromigration (EM) Budget Validation — P21
//!
//! Validates that the trace cross-section (W × T) is physically wide enough to
//! support the user's declared current budget (I_budget) against the material's
//! `max_current_density` (J_max).
//!
//! **The One Physical Question:**
//!   J_budget = I_budget / (Width × Thickness)
//!   If J_budget > J_max, the trace is physically too narrow to carry the
//!   budget declared by the user.
//!
//! **What This Check Does NOT Do:**
//! - ❌ Calculate actual operating currents (requires SPICE matrix solver)
//! - ❌ Solve I = V/R for the circuit (belongs to the simulation layer)
//! - ❌ Validate simulated currents vs. wire capability (dynamic sign-off, P21-D)
//!
//! **Physical Phenomenon (For Reference):**
//! - High-velocity electrons physically collide with metal atoms (electron wind)
//! - Metal atoms migrate downstream, causing:
//!   - Voids (where atoms leave) → Open circuits
//!   - Hillocks/whiskers (where atoms accumulate) → Short circuits
//!
//! **Governing Equation:** Black's Equation
//!   MTTF = A / J^n * exp(Ea / kT)
//!   Mean Time To Failure decreases exponentially with current density J and temperature T
//!
//! **Typical Limits:**
//! - Aluminum: 1.0 mA/μm² (1000 A/mm²)
//! - Copper: 2.0 mA/μm² (2000 A/mm²)
//! - Polysilicon: 0.1 mA/μm² (100 A/mm²)
//!
//! **See:** ELECTROMIGRATION-AND-THERMAL.md for the three-tier architecture explanation.

use crate::geometry::Point3D;
use crate::material::MaterialRegistry;
use crate::space::AnalyticTrace;

use super::types::DrcViolation;

/// Validate electromigration constraints for all analytic routes (STATIC BUDGET CHECK).
///
/// **ARCHITECTURAL BOUNDARY:** This function validates the DECLARED BUDGET against
/// the trace GEOMETRY, not simulated operating points. `current.budget_ma` stores the
/// user's declared budget from `nets: { current: X }`, NOT a computed operating current.
///
/// Single physical check:
///   J_budget = I_budget / (Width × Thickness) ≤ material.max_current_density
///
/// **Dynamic validation (simulated currents) belongs to post-simulation sign-off (P21-D).**
pub fn validate_electromigration(
    routes: &[AnalyticTrace],
    material_registry: &MaterialRegistry,
) -> Result<Vec<DrcViolation>, String> {
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let chunk_size = (routes.len() + cpu_cores - 1).max(1);

    // Each thread validates a contiguous chunk of routes, returning either a
    // partial violation list or an error (propagated out of the scope).
    let partial_results: Vec<Result<Vec<DrcViolation>, String>> = std::thread::scope(|s| {
        let mut handles = Vec::new();

        for chunk in routes.chunks(chunk_size) {
            let handle = s.spawn(move || {
                let mut local_violations: Vec<DrcViolation> = Vec::new();
                for route in chunk {
                    // Skip unconstrained/signal nets with no declared budget (Artist Mode)
                    let budget_ma = route.current.budget_ma;
                    if budget_ma <= 0.0 {
                        continue;
                    }

                    let width_nm = route.cross_section.width_nm as f64;
                    let thickness_nm = route.cross_section.thickness_nm as f64;
                    let area_nm2 = width_nm * thickness_nm;

                    if area_nm2 <= 0.0 {
                        return Err(format!(
                            "[DRC EM] FATAL: trace on net '{}' has zero cross-sectional area (width={}nm, thickness={}nm).",
                            route.net_name, width_nm, thickness_nm
                        ));
                    }

                    let props = material_registry.get_physical_props(route.material)
                        .ok_or_else(|| format!(
                            "[DRC EM] FATAL: material_id {} not found in registry for net '{}'.",
                            route.material, route.net_name
                        ))?;

                    let max_density_a_mm2 = props.get("max_current_density").ok_or_else(|| format!(
                        "[DRC EM] FATAL: material for net '{}' has no 'max_current_density' property. \
                         Add max_current_density to your material definition.",
                        route.net_name
                    ))?;

                    // Convert to SI-consistent units for the density comparison.
                    let budget_a = budget_ma * 1e-3;
                    let area_mm2 = area_nm2 * 1e-12; // nm² → mm²
                    let budget_density_a_mm2 = budget_a / area_mm2;

                    
                    if budget_density_a_mm2 > max_density_a_mm2 {
                        let location = route
                            .segments
                            .first()
                            .map(|s| s.start)
                            .unwrap_or(Point3D::new(0, 0, 0));

                        local_violations.push(DrcViolation::ElectromigrationViolation {
                            net: route.net_name.clone(),
                            actual_density_a_mm2: budget_density_a_mm2,
                            max_density_a_mm2,
                            location,
                        });
                    }
                }

                Ok(local_violations)
            });
            handles.push(handle);
        }

        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    // Merge all violations, propagating errors.
    let mut violations = Vec::new();
    for partial in partial_results {
        violations.extend(partial?);
    }

    Ok(violations)
}
