//! Current density validation — validates actual current against route capability and material limits.
//!
//! v0.1.8: Fixed fundamental architecture flaw. Now validates:
//! 1. Actual operating current ≤ route's declared capability
//! 2. Route's capability ≤ material's physical limit (given geometry)

use crate::geometry::Point3D;
use crate::material::MaterialRegistry;
use crate::space::AnalyticTrace;

use super::types::DrcViolation;

/// Validate current density for all analytic routes.
///
/// Implements two-tier validation:
/// 1. **Netlist check**: actual_current ≤ route.current_limit
/// 2. **Physical check**: route.current_limit ≤ material.max_density × cross_section
///
/// This ensures both electrical correctness (nets don't exceed trace capacity)
/// and manufacturing feasibility (traces can physically handle their declared capacity).
pub fn validate_current_density(
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
                            "[DRC] FATAL: trace on net '{}' has zero cross-sectional area (width={}nm, thickness={}nm).",
                            route.net_name, route.cross_section.width_nm, route.cross_section.thickness_nm
                        ));
                    }

                    let props = material_registry.get_physical_props(route.material)
                        .ok_or_else(|| format!(
                            "[DRC] FATAL: material_id {} not found in registry for net '{}'.",
                            route.material, route.net_name
                        ))?;
                    let resistivity = props.get("resistivity").ok_or_else(|| format!(
                        "[DRC] FATAL: material for net '{}' has no 'resistivity' property. \
                         Add resistivity to your material definition.",
                        route.net_name
                    ))?;
                    let thermal_k = props.get("thermal_conductivity").ok_or_else(|| format!(
                        "[DRC] FATAL: material for net '{}' has no 'thermal_conductivity' property. \
                         Add thermal_conductivity to your material definition.",
                        route.net_name
                    ))?;
                    let max_density_a_mm2 = props.get("max_current_density").ok_or_else(|| format!(
                        "[DRC] FATAL: material for net '{}' has no 'max_current_density' property. \
                         Add max_current_density to your material definition.",
                        route.net_name
                    ))?;

                    eprintln!("[DRC THERMAL DEBUG] Found props for material {} (net '{}'): resistivity={}, thermal_k={}, max_i={}", 
                        route.material, route.net_name, resistivity, thermal_k, max_density_a_mm2);
                    // CHECK 1: Does actual operating current exceed the route's declared capability?
                    if route.current.actual_ma > route.current.limit_ma {
                        let location = route
                            .segments
                            .first()
                            .map(|s| s.start)
                            .unwrap_or(Point3D::new(0, 0, 0));

                        local_violations.push(DrcViolation::CurrentDensityViolation {
                            net: route.net_name.clone(),
                            actual_density_a_mm2: route.current.actual_ma,
                            max_density_a_mm2: route.current.limit_ma,
                            location,
                        });
                        continue;
                    }

                    // CHECK 2: Does the route's declared capability exceed the material's physical limit?
                    // Calculate maximum current the geometry can physically handle
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

                        local_violations.push(DrcViolation::CurrentDensityViolation {
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
