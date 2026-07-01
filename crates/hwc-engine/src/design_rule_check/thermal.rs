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
    use rayon::prelude::*;

    let violations = routes
        .par_iter()
        .filter_map(|route| -> Option<Result<DrcViolation, String>> {
            // Skip routes with no current limit declared (Artist Mode)
            if route.current_limit_ma <= 0.0 {
                return None;
            }

            let area_nm2 = route.width_nm as f64 * route.thickness_nm as f64;
            if area_nm2 <= 0.0 {
                return Some(Err(format!(
                    "[DRC] FATAL: trace on net '{}' has zero cross-sectional area (width={}nm, thickness={}nm).",
                    route.net_name, route.width_nm, route.thickness_nm
                )));
            }

            let props = match material_registry.get_physical_props(route.material) {
                Some(p) => p,
                None => return Some(Err(format!(
                    "[DRC] FATAL: material_id {} not found in registry for net '{}'.",
                    route.material, route.net_name
                ))),
            };

            let max_density_a_mm2 = match props.max_current_density_a_mm2 {
                Some(d) => d,
                None => return Some(Err(format!(
                    "[DRC] FATAL: material for net '{}' has no max_current_density in materials.hw. \
                     Add max_current_density to your material definition.",
                    route.net_name
                ))),
            };

            // CHECK 1: Does actual operating current exceed the route's declared capability?
            if route.current_ma > route.current_limit_ma {
                let location = route
                    .segments
                    .first()
                    .map(|s| s.start)
                    .unwrap_or(Point3D::new(0, 0, 0));

                return Some(Ok(DrcViolation::CurrentDensityViolation {
                    net: route.net_name.clone(),
                    actual_density_a_mm2: route.current_ma,
                    max_density_a_mm2: route.current_limit_ma,
                    location,
                }));
            }

            // CHECK 2: Does the route's declared capability exceed the material's physical limit?
            // Calculate maximum current the geometry can physically handle
            let current_limit_a = route.current_limit_ma / 1000.0;
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

                Some(Ok(DrcViolation::CurrentDensityViolation {
                    net: route.net_name.clone(),
                    actual_density_a_mm2: capability_density_a_mm2,
                    max_density_a_mm2: max_density_a_mm2,
                    location,
                }))
            } else {
                None
            }
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(violations)
}
