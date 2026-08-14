//! Electromigration (EM) validation — P21: Physical metal atom migration under high current density.
//!
//! **Physical Phenomenon:**
//! - High-velocity electrons physically collide with metal atoms (electron wind)
//! - Metal atoms migrate downstream, causing:
//!   - Voids (where atoms leave) → Open circuits
//!   - Hillocks/whiskers (where atoms accumulate) → Short circuits
//!
//! **Governing Equation:** Black's Equation
//!   MTTF = A / J^n * exp(Ea / kT)
//!   Mean Time To Failure decreases exponentially with current density J and temperature T
//!
//! **Check:** J_DC = I_peak / (Width × Thickness) ≤ Material.max_current_density
//!
//! **Typical Limits:**
//! - Aluminum: 1.0 mA/μm² (1000 A/mm²)
//! - Copper: 2.0 mA/μm² (2000 A/mm²)
//! - Polysilicon: 0.1 mA/μm² (100 A/mm²)

use crate::geometry::Point3D;
use crate::material::MaterialRegistry;
use crate::space::AnalyticTrace;

use super::types::DrcViolation;

/// Validate electromigration constraints for all analytic routes.
///
/// Implements two-tier validation:
/// 1. **Netlist check**: actual_current ≤ route.current_limit
/// 2. **Physical check**: route.current_limit ≤ material.max_density × cross_section
///
/// This ensures both electrical correctness (nets don't exceed trace capacity)
/// and manufacturing feasibility (traces can physically handle their declared capacity).
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
                    // Skip routes with no current limit declared (Artist Mode)
                    if route.current.limit_ma <= 0.0 {
                        continue;
                    }

                    let area_nm2 = route.cross_section.width_nm as f64
                        * route.cross_section.thickness_nm as f64;
                    if area_nm2 <= 0.0 {
                        return Err(format!(
                            "[DRC EM] FATAL: trace on net '{}' has zero cross-sectional area (width={}nm, thickness={}nm).",
                            route.net_name, route.cross_section.width_nm, route.cross_section.thickness_nm
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

                    eprintln!("[DRC EM DEBUG] Checking net '{}': material_id={}, max_density={:.2} A/mm²", 
                        route.net_name, route.material, max_density_a_mm2);

                    // CHECK 1: Does actual operating current exceed the route's declared capability?
                    if route.current.actual_ma > route.current.limit_ma {
                        let location = route
                            .segments
                            .first()
                            .map(|s| s.start)
                            .unwrap_or(Point3D::new(0, 0, 0));

                        local_violations.push(DrcViolation::ElectromigrationViolation {
                            net: route.net_name.clone(),
                            actual_density_a_mm2: route.current.actual_ma,
                            max_density_a_mm2: route.current.limit_ma,
                            location,
                        });
                        continue;
                    }

                    // CHECK 2: Does the route's declared capability exceed the material's physical limit?
                    // Calculate maximum current density the geometry can physically handle
                    let current_limit_a = route.current.limit_ma / 1000.0;
                    let area_m2 = area_nm2 * 1e-18; // nm² → m²
                    let capability_density_a_m2 = current_limit_a / area_m2;
                    let capability_density_a_mm2 = capability_density_a_m2 / 1e6;

                    // Convert material limit to A/m² for comparison
                    let max_density_a_m2 = max_density_a_mm2 * 1e6;

                    if capability_density_a_m2 > max_density_a_m2 {
                        let location = route
                            .segments
                            .first()
                            .map(|s| s.start)
                            .unwrap_or(Point3D::new(0, 0, 0));

                        eprintln!("[DRC EM DEBUG] • EM violation for {}: {:.2} A/mm² actual, {:.2} A/mm² max", 
                            route.net_name, capability_density_a_mm2, max_density_a_mm2);

                        local_violations.push(DrcViolation::ElectromigrationViolation {
                            net: route.net_name.clone(),
                            actual_density_a_mm2: capability_density_a_mm2,
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
