//! Thermal validation logic.

use crate::constraint_manager::ConstraintRulebook;
use crate::geometry::Point3D;

use super::types::{DrcViolation, MaterialProperties, NetVoxels};

/// Validate thermal properties for all nets.
///
/// **TRUE DATA PARALLELISM**: Uses Rayon to parallelize over nets (the massive dataset),
/// not validators (the 3 functions). This spreads work across all CPU cores.
///
/// **Algorithm**:
/// 1. For each net (in parallel across all CPU cores)
/// 2. Calculate power dissipation (I²R)
/// 3. Calculate temperature rise
/// 4. If temperature > max → violation
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 200-300, thermal calculations)
///
/// # Arguments
/// * `nets` - All routed nets with their voxel locations
/// * `constraints` - Constraint rulebook with thermal limits
/// * `material` - Material properties for thermal calculations
/// * `voxel_size_nm` - Size of one voxel in nanometers
///
/// # Returns
/// Vector of thermal violations
pub fn validate_thermal(
    nets: &[NetVoxels],
    constraints: &ConstraintRulebook,
    material: &MaterialProperties,
    voxel_size_nm: i64,
) -> Vec<DrcViolation> {
    use rayon::prelude::*;

    // eprintln!($3"[DEBUG THERMAL] Starting thermal validation for {} nets", nets.len());

    // Get constraints once (avoid repeated lookups)
    let current_ma = constraints.default_current_ma.unwrap_or(20);
    let max_temp_rise = constraints.max_temp_rise_c.unwrap_or(10.0);

    // eprintln!($3"[DEBUG THERMAL] Constraints: current={}mA, max_temp_rise={}°C", current_ma, max_temp_rise);

    // TRUE PARALLELISM: Parallelize over nets (10,000+ items) not validators (3 items)
    // This uses ALL CPU cores efficiently
    nets.par_iter()
        .filter_map(|net| {
            // PROPER GEOMETRY-AWARE THERMAL VALIDATION (v0.1.6)
            // Apply appropriate thermal model based on geometry type
            use super::types::GeometryType;

            let temp_rise = match net.geometry_type {
                GeometryType::Trace => {
                    // 1D TRACE THERMAL MODEL
                    // Physics: Linear conductor with I²R heating
                    // Temperature rise: ΔT = P / (k × A) where P = I²R
                    let trace_length_nm = calculate_trace_length(&net.voxels, voxel_size_nm);
                    let trace_width_nm = voxel_size_nm;

                    calculate_temperature_rise(
                        current_ma,
                        trace_length_nm,
                        trace_width_nm,
                        material,
                    )
                }
                GeometryType::Pour => {
                    // 2D POUR THERMAL MODEL
                    // Physics: Large copper area with surface heat dissipation
                    // Pours have much better thermal performance than traces due to:
                    // 1. Large cross-sectional area (low resistance)
                    // 2. Large surface area (efficient heat dissipation)
                    // 3. Thermal spreading (heat distributes across entire pour)
                    calculate_pour_temperature_rise(
                        current_ma,
                        &net.voxels,
                        voxel_size_nm,
                        material,
                    )
                }
                GeometryType::Contact => {
                    // 3D CONTACT/VIA THERMAL MODEL
                    // Physics: Vertical conductor with thermal resistance
                    // Vias have concentrated current through small cross-section
                    // Critical for high-current applications
                    calculate_via_temperature_rise(current_ma, &net.voxels, voxel_size_nm, material)
                }
            };

            // Only return violations
            if temp_rise > max_temp_rise {
                let location = net.voxels.first().copied().unwrap_or(Point3D::new(0, 0, 0));
                // eprintln!($3"[DEBUG THERMAL] VIOLATION: Net '{}' temp rise {:.2}°C > max {:.2}°C", net.net_name, temp_rise, max_temp_rise);
                Some(DrcViolation::ThermalViolation {
                    net: net.net_name.clone(),
                    temperature_c: temp_rise,
                    max_c: max_temp_rise,
                    location,
                })
            } else {
                // eprintln!($3"[DEBUG THERMAL] OK: Net '{}' temp rise {:.2}°C <= max {:.2}°C", net.net_name, temp_rise, max_temp_rise);
                None
            }
        })
        .collect()
}

/// Calculate trace length from voxel path.
///
/// **O(N) Algorithm**:
/// For accurate trace length, we need to sum the distances between consecutive voxels.
/// This is more accurate than just counting voxels.
pub fn calculate_trace_length(voxels: &[Point3D], _voxel_size_nm: i64) -> i64 {
    if voxels.len() <= 1 {
        return 0;
    }

    // Sum Manhattan distances between consecutive voxels
    let mut total_length = 0;
    for i in 1..voxels.len() {
        total_length += voxels[i].manhattan_distance(&voxels[i - 1]);
    }

    total_length
}

/// Calculate temperature rise for a trace.
///
/// Uses simplified thermal model:
/// 1. Calculate resistance: R = ρ × (L / A)
/// 2. Calculate power: P = I² × R
/// 3. Estimate temperature rise (simplified)
///
/// **Algorithm**:
/// - Resistance: R = resistivity × length / area
/// - Power: P = I² × R
/// - Temperature rise: ΔT ≈ P / (thermal_conductivity × area)
///
/// # Arguments
/// * `current_ma` - Current in milliamps
/// * `trace_length_nm` - Trace length in nanometers
/// * `trace_width_nm` - Trace width in nanometers
/// * `material` - Material properties
///
/// # Returns
/// Temperature rise in Celsius
pub fn calculate_temperature_rise(
    current_ma: i64,
    trace_length_nm: i64,
    trace_width_nm: i64,
    material: &MaterialProperties,
) -> f64 {
    // Convert current to amps
    let current_a = current_ma as f64 / 1000.0;

    // Assume 1oz copper thickness (35 micrometers = 35,000 nm)
    let thickness_nm = 35_000.0;

    // Calculate cross-sectional area (width × thickness)
    let area_nm2 = trace_width_nm as f64 * thickness_nm;

    // Calculate resistance: R = ρ × (L / A)
    let resistance_ohm = material.resistivity_ohm_nm * (trace_length_nm as f64 / area_nm2);

    // Calculate power dissipation: P = I² × R
    let power_w = current_a * current_a * resistance_ohm;

    // Simplified temperature rise calculation
    // ΔT ≈ P / (k × A) where k is thermal conductivity
    // Convert area from nm² to m² for thermal conductivity units
    let area_m2 = area_nm2 / 1e18;
    let temp_rise_c = power_w / (material.thermal_conductivity * area_m2);

    // Clamp to reasonable values
    temp_rise_c.clamp(0.0, 1000.0)
}

/// Calculate temperature rise for a 2D copper pour.
///
/// **2D Pour Thermal Model**:
/// Pours have significantly better thermal performance than traces:
/// 1. Large cross-sectional area → Low resistance → Low I²R heating
/// 2. Large surface area → Efficient heat dissipation to ambient/substrate
/// 3. Thermal spreading → Heat distributes across entire pour area
///
/// **Simplified Model**:
/// - Calculate effective resistance based on pour volume
/// - Apply surface area heat dissipation factor
/// - Pours typically run 10-100× cooler than equivalent traces
///
/// # Arguments
/// * `current_ma` - Current in milliamps
/// * `voxels` - Voxel positions defining the pour geometry
/// * `voxel_size_nm` - Size of one voxel in nanometers
/// * `material` - Material properties
///
/// # Returns
/// Temperature rise in Celsius
pub fn calculate_pour_temperature_rise(
    current_ma: i64,
    voxels: &[Point3D],
    voxel_size_nm: i64,
    material: &MaterialProperties,
) -> f64 {
    if voxels.is_empty() || current_ma == 0 {
        return 0.0;
    }

    // Convert current to amps
    let current_a = current_ma as f64 / 1000.0;

    // Calculate pour dimensions from voxel bounding box
    let (min_x, max_x) = voxels
        .iter()
        .map(|v| v.x)
        .fold((i64::MAX, i64::MIN), |(min, max), x| {
            (min.min(x), max.max(x))
        });
    let (min_y, max_y) = voxels
        .iter()
        .map(|v| v.y)
        .fold((i64::MAX, i64::MIN), |(min, max), y| {
            (min.min(y), max.max(y))
        });
    let (min_z, max_z) = voxels
        .iter()
        .map(|v| v.z)
        .fold((i64::MAX, i64::MIN), |(min, max), z| {
            (min.min(z), max.max(z))
        });

    let width_nm = (max_x - min_x).max(voxel_size_nm) as f64;
    let height_nm = (max_y - min_y).max(voxel_size_nm) as f64;
    let thickness_nm = (max_z - min_z).max(voxel_size_nm) as f64;

    // Calculate cross-sectional area (perpendicular to current flow)
    // For a pour, assume current flows across the width, so area = height × thickness
    let area_nm2 = height_nm * thickness_nm;

    // Calculate resistance: R = ρ × (L / A)
    // For a pour, effective length is the width (current spreads across the pour)
    let resistance_ohm = material.resistivity_ohm_nm * (width_nm / area_nm2);

    // Calculate power dissipation: P = I² × R
    let power_w = current_a * current_a * resistance_ohm;

    // Calculate surface area for heat dissipation (top + bottom surfaces)
    let surface_area_nm2 = 2.0 * width_nm * height_nm;
    let surface_area_m2 = surface_area_nm2 / 1e18;

    // Temperature rise with surface area heat dissipation
    // Pours dissipate heat much more efficiently than traces
    // Apply a thermal spreading factor (pours spread heat across entire area)
    let thermal_spreading_factor = 0.1; // Pours run ~10× cooler due to spreading
    let temp_rise_c =
        (power_w / (material.thermal_conductivity * surface_area_m2)) * thermal_spreading_factor;

    // Clamp to reasonable values
    temp_rise_c.clamp(0.0, 1000.0)
}

/// Calculate temperature rise for a 3D contact/via.
///
/// **3D Via Thermal Model**:
/// Vias are vertical conductors with unique thermal characteristics:
/// 1. Small cross-sectional area → Higher resistance than pours
/// 2. Vertical current flow → Concentrated heating
/// 3. Limited surface area → Less efficient heat dissipation
/// 4. Thermal resistance to substrate → Heat buildup
///
/// **Critical for**:
/// - High-current power delivery
/// - Thermal vias for heat dissipation
/// - Via arrays for current sharing
///
/// **Simplified Model**:
/// - Calculate via resistance based on diameter and height
/// - Apply via-specific thermal resistance
/// - Vias typically run hotter than pours but cooler than thin traces
///
/// # Arguments
/// * `current_ma` - Current in milliamps
/// * `voxels` - Voxel positions defining the via geometry
/// * `voxel_size_nm` - Size of one voxel in nanometers
/// * `material` - Material properties
///
/// # Returns
/// Temperature rise in Celsius
pub fn calculate_via_temperature_rise(
    current_ma: i64,
    voxels: &[Point3D],
    voxel_size_nm: i64,
    material: &MaterialProperties,
) -> f64 {
    if voxels.is_empty() || current_ma == 0 {
        return 0.0;
    }

    // Convert current to amps
    let current_a = current_ma as f64 / 1000.0;

    // Calculate via dimensions from voxel bounding box
    let (min_x, max_x) = voxels
        .iter()
        .map(|v| v.x)
        .fold((i64::MAX, i64::MIN), |(min, max), x| {
            (min.min(x), max.max(x))
        });
    let (min_y, max_y) = voxels
        .iter()
        .map(|v| v.y)
        .fold((i64::MAX, i64::MIN), |(min, max), y| {
            (min.min(y), max.max(y))
        });
    let (min_z, max_z) = voxels
        .iter()
        .map(|v| v.z)
        .fold((i64::MAX, i64::MIN), |(min, max), z| {
            (min.min(z), max.max(z))
        });

    // Via diameter (average of X and Y dimensions)
    let diameter_x_nm = (max_x - min_x).max(voxel_size_nm) as f64;
    let diameter_y_nm = (max_y - min_y).max(voxel_size_nm) as f64;
    let diameter_nm = (diameter_x_nm + diameter_y_nm) / 2.0;

    // Via height (Z dimension)
    let height_nm = (max_z - min_z).max(voxel_size_nm) as f64;

    // Calculate via cross-sectional area (circular approximation)
    let radius_nm = diameter_nm / 2.0;
    let area_nm2 = std::f64::consts::PI * radius_nm * radius_nm;

    // Calculate resistance: R = ρ × (L / A)
    let resistance_ohm = material.resistivity_ohm_nm * (height_nm / area_nm2);

    // Calculate power dissipation: P = I² × R
    let power_w = current_a * current_a * resistance_ohm;

    // Calculate via surface area for heat dissipation (cylindrical surface)
    let surface_area_nm2 = 2.0 * std::f64::consts::PI * radius_nm * height_nm;
    let surface_area_m2 = surface_area_nm2 / 1e18;

    // Temperature rise for via
    // Vias have moderate thermal performance (better than thin traces, worse than pours)
    // Apply via thermal resistance factor
    let via_thermal_factor = 0.5; // Vias run ~2× cooler than equivalent traces
    let temp_rise_c =
        (power_w / (material.thermal_conductivity * surface_area_m2)) * via_thermal_factor;

    // Clamp to reasonable values
    temp_rise_c.clamp(0.0, 1000.0)
}
