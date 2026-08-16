//! Thermal rise validation — P22: STATIC BUDGET vs. Thermal Capacity Check
//!
//! **CRITICAL ARCHITECTURAL BOUNDARY:**
//! This module validates DECLARED CURRENT BUDGETS vs. THERMAL CAPACITY, NOT simulated
//! operating points. The `current` field in `.hw` files is a DESIGN CONSTRAINT, not
//! a computed operating current.
//!
//! **What This Check Does:**
//! - ✅ Validates: "If this trace carried its declared budget, would self-heating be safe?"
//! - ✅ Formula: ΔT_budget = (I_declared² × R) / (k × Surface_Area)
//! - ✅ Failure: "Declared budget would cause excessive temperature rise"
//!
//! **What This Check Does NOT Do:**
//! - ❌ Calculate actual operating currents (requires SPICE matrix solver)
//! - ❌ Solve I = V/R for the circuit (belongs to simulation layer)
//! - ❌ Validate simulated power dissipation (belongs to dynamic sign-off)
//!
//! **Physical Phenomenon (For Reference):**
//! - High RMS current through resistive traces generates Joule heat: P = I²R
//! - Heat trapped in dielectric layers causes local temperature rise: ΔT
//! - Excessive ΔT causes:
//!   - Dielectric delamination
//!   - Dopant drift in semiconductors
//!   - Thermal runaway in high-resistance materials
//!
//! **Governing Equations:**
//!   R = ρ × (L / A)               [Trace resistance]
//!   P = I_budget² × R             [Power if trace carries declared budget]
//!   ΔT = P / (k × Surface_Area)   [1D substrate diffusion model]
//!
//! **Static Check:** ΔT_budget ≤ Profile.max_temp_rise (user-declared, e.g., 20°C)
//!
//! **No Defaults:** The profile MUST declare max_temp_rise explicitly.
//!
//! **See:** ELECTROMIGRATION-AND-THERMAL.md for the three-tier architecture explanation.

use crate::geometry::Point3D;
use crate::material::MaterialRegistry;
use crate::space::AnalyticTrace;

use super::types::DrcViolation;

/// Validate thermal rise (ΔT) for all analytic routes (STATIC BUDGET CHECK).
///
/// **ARCHITECTURAL BOUNDARY:** This function validates DECLARED BUDGETS vs. THERMAL CAPACITY,
/// not simulated operating points. The `current.budget_ma` field stores the user's
/// declared budget from `nets: { current: X }`, NOT a computed operating current.
///
/// Calculates hypothetical self-heating if the trace carried its declared budget current:
///   P_budget = I_declared² × R
///   ΔT_budget = P_budget / (k × Surface_Area)
///
/// This is separate from electromigration (P21) which checks current density limits.
///
/// **Dynamic validation (simulated power dissipation) belongs to post-simulation sign-off (P22-D).**
///
/// # Arguments
/// * `routes` - All analytic traces in the design
/// * `material_registry` - Material properties (resistivity, thermal_conductivity)
/// * `max_temp_rise_c` - Maximum allowed temperature rise in °C (from profile, REQUIRED)
pub fn validate_thermal_rise(
    routes: &[AnalyticTrace],
    material_registry: &MaterialRegistry,
    max_temp_rise_c: f64,
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
                    // Skip routes with no current budget declared (Artist Mode / NC nets)
                    // NOTE: route.current.budget_ma is the DECLARED BUDGET from nets: { current: X },
                    //       NOT a simulated operating current. This is a static capacity check.
                    let budget_ma = route.current.budget_ma;
                    if budget_ma <= 0.0 {
                        continue;
                    }

                    // Calculate trace geometry
                    let width_nm = route.cross_section.width_nm as f64;
                    let thickness_nm = route.cross_section.thickness_nm as f64;
                    let area_nm2 = width_nm * thickness_nm;
                    
                    if area_nm2 <= 0.0 {
                        return Err(format!(
                            "[DRC THERMAL] FATAL: trace on net '{}' has zero cross-sectional area (width={}nm, thickness={}nm).",
                            route.net_name, width_nm, thickness_nm
                        ));
                    }

                    // Calculate trace length (sum of all segment lengths)
                    let length_nm: f64 = route.segments.iter()
                        .map(|seg| {
                            let dx = (seg.end.x - seg.start.x) as f64;
                            let dy = (seg.end.y - seg.start.y) as f64;
                            let dz = (seg.end.z - seg.start.z) as f64;
                            (dx * dx + dy * dy + dz * dz).sqrt()
                        })
                        .sum();

                    if length_nm <= 0.0 {
                        continue; // Zero-length trace, no thermal concern
                    }

                    // Get material properties
                    let props = material_registry.get_physical_props(route.material)
                        .ok_or_else(|| format!(
                            "[DRC THERMAL] FATAL: material_id {} not found in registry for net '{}'.",
                            route.material, route.net_name
                        ))?;
                    
                    let resistivity = props.get("resistivity").ok_or_else(|| format!(
                        "[DRC THERMAL] FATAL: material for net '{}' has no 'resistivity' property. \
                         Add resistivity to your material definition.",
                        route.net_name
                    ))?;
                    
                    let thermal_k = props.get("thermal_conductivity").ok_or_else(|| format!(
                        "[DRC THERMAL] FATAL: material for net '{}' has no 'thermal_conductivity' property. \
                         Add thermal_conductivity to your material definition.",
                        route.net_name
                    ))?;

                    eprintln!("[DRC THERMAL DEBUG] Checking net '{}': material_id={}, ρ={:.2e} Ω·m, k={} W/(m·K)", 
                        route.net_name, route.material, resistivity, thermal_k);

                    // Convert to SI units
                    let length_m = length_nm * 1e-9;
                    let width_m = width_nm * 1e-9;
                    let _thickness_m = thickness_nm * 1e-9; // Unused but kept for clarity
                    let area_m2 = area_nm2 * 1e-18;

                    // STEP 1: Calculate trace resistance: R = ρ × (L / A)
                    let resistance_ohms = resistivity * (length_m / area_m2);

                    // STEP 2: Calculate hypothetical power if trace carried its declared budget
                    // NOTE: This is NOT simulated power. It's "what if the trace carried its budget?"
                    //       P_budget = I_declared² × R
                    let current_budget_a = budget_ma * 1e-3;
                    let power_watts = current_budget_a * current_budget_a * resistance_ohms;

                    // STEP 3: Calculate temperature rise using 1D substrate diffusion model
                    // ΔT = P / (k × Surface_Area)
                    // Surface area = 2 × length × width (top and bottom heat dissipation)
                    let surface_area_m2 = 2.0 * length_m * width_m;
                    
                    if surface_area_m2 <= 0.0 {
                        continue; // Degenerate geometry
                    }

                    let delta_t_celsius = power_watts / (thermal_k * surface_area_m2);

                    eprintln!("[DRC THERMAL DEBUG] • Net '{}': R={:.2e}Ω, P_budget={:.2e}W, ΔT_budget={:.2}°C (limit: {:.2}°C)", 
                        route.net_name, resistance_ohms, power_watts, delta_t_celsius, max_temp_rise_c);

                    // STEP 4: Check hypothetical temperature rise against thermal budget
                    if delta_t_celsius > max_temp_rise_c {
                        let location = route
                            .segments
                            .first()
                            .map(|s| s.start)
                            .unwrap_or(Point3D::new(0, 0, 0));

                        eprintln!("[DRC THERMAL DEBUG] • STATIC THERMAL VIOLATION for {}: ΔT_budget={:.2}°C > {:.2}°C", 
                            route.net_name, delta_t_celsius, max_temp_rise_c);

                        local_violations.push(DrcViolation::ThermalRiseViolation {
                            net: route.net_name.clone(),
                            actual_temp_rise_c: delta_t_celsius,
                            max_temp_rise_c,
                            power_uw: power_watts * 1e6,
                            resistance_ohms,
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
