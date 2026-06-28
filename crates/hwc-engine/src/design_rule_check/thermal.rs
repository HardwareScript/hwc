//! Thermal validation logic.
//!
//! v0.1.8: Rewritten to use analytic routes instead of voxel geometry.
//! Uses per-route current limits and material properties from the symbol table.

use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::Point3D;
use crate::material::{MaterialId, MaterialRegistry, MaterialPhysicalProps};
use crate::space::AnalyticTrace;

use super::types::DrcViolation;

/// Validate thermal properties for all analytic routes.
///
/// Uses analytic geometry (no voxels) with per-route current and material lookup.
///
/// **Algorithm**:
/// 1. For each analytic route (in parallel)
/// 2. Look up material properties from the registry
/// 3. Use per-route current from `current_limit` declaration
/// 4. Calculate I²R temperature rise
/// 5. If temperature rise > max_temp_rise → violation
pub fn validate_thermal_analytic(
    routes: &[AnalyticTrace],
    constraints: &ConstraintRulebook,
    material_registry: &MaterialRegistry,
) -> Result<Vec<DrcViolation>, String> {
    use rayon::prelude::*;

    let max_temp_rise = constraints.max_temp_rise_c
        .ok_or_else(|| "[DRC] FATAL: max_temp_rise_c not set in constraint rulebook. Add thermal constraints to your PDK profile.".to_string())?;

    let violations = routes
        .par_iter()
        .map(|route| -> Result<Option<DrcViolation>, String> {
            let current_ma = if route.current_ma > 0.0 {
                route.current_ma
            } else {
                constraints.default_current_ma
                    .ok_or_else(|| format!(
                        "[DRC] FATAL: net '{}' has no current_limit and no default_current_ma in constraint rulebook. Add current_limit to your route declaration or default_current_ma to your PDK profile.",
                        route.net_name
                    ))? as f64
            };

            let material_props = lookup_material_props(route.material, material_registry)?;

            let total_length_nm: i64 = route.segments.iter().map(|s| s.length()).sum();

            if total_length_nm <= 0 {
                return Ok(None);
            }

            let temp_rise = calculate_trace_temperature_rise(
                current_ma,
                total_length_nm,
                route.width_nm,
                route.thickness_nm,
                &material_props,
            )?;

            if temp_rise > max_temp_rise {
                let location = route
                    .segments
                    .first()
                    .map(|s| s.start)
                    .unwrap_or(Point3D::new(0, 0, 0));

                Ok(Some(DrcViolation::ThermalViolation {
                    net: route.net_name.clone(),
                    temperature_c: temp_rise,
                    max_c: max_temp_rise,
                    location,
                }))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<Option<_>>, String>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(violations)
}

/// Look up material physical properties from the registry.
fn lookup_material_props(
    material_id: MaterialId,
    registry: &MaterialRegistry,
) -> Result<MaterialPhysicalProps, String> {
    registry.get_physical_props(material_id)
        .ok_or_else(|| format!(
            "[DRC] FATAL: material_id {} not found in registry. Ensure all materials are declared in your materials.hw file.",
            material_id
        ))
}

/// Calculate temperature rise for a trace using I²R physics.
///
/// Formula: ΔT = I² × ρ × L / (k × A²)
/// Where:
///   I = current (Amps)
///   ρ = resistivity (Ω·m)
///   L = trace length (m)
///   k = thermal conductivity (W/(m·K))
///   A = cross-sectional area (m²) = width × thickness
fn calculate_trace_temperature_rise(
    current_ma: f64,
    length_nm: i64,
    width_nm: i64,
    thickness_nm: i64,
    props: &MaterialPhysicalProps,
) -> Result<f64, String> {
    let current_a = current_ma / 1000.0;
    let length_m = length_nm as f64 * 1e-9;
    let width_m = width_nm as f64 * 1e-9;
    let thickness_m = thickness_nm as f64 * 1e-9;

    let area_m2 = width_m * thickness_m;

    if area_m2 <= 0.0 {
        return Err(format!(
            "[DRC] FATAL: trace has zero cross-sectional area (width={}nm, thickness={}nm). \
             Declare a non-zero width and thickness for all traces.",
            width_nm, thickness_nm
        ));
    }
    if props.thermal_conductivity_w_mk <= 0.0 {
        return Err(format!(
            "[DRC] FATAL: material has zero thermal conductivity. \
             Ensure resistivity_ohm_m and thermal_conductivity_w_mk are set in your materials.hw file."
        ));
    }

    // R = ρ × L / A
    let resistance_ohm = props.resistivity_ohm_m * (length_m / area_m2);

    // P = I² × R
    let power_w = current_a * current_a * resistance_ohm;

    // ΔT = P / (k × A)
    let temp_rise_c = power_w / (props.thermal_conductivity_w_mk * area_m2);

    Ok(temp_rise_c.clamp(0.0, 1000.0))
}

