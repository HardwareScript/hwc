//! Dielectric breakdown to clearance translation.
//!
//! This module implements Phase 1.2 of the constraint generation pipeline,
//! converting voltage differences and material properties into geometric
//! clearance requirements.

use crate::geometry::Point3D;

// ============================================================================
// Phase 1.2: Dielectric Breakdown to Clearance Translation
// ============================================================================

/// Calculate minimum clearance from voltage difference and material properties.
///
/// Uses the formula: clearance = (voltage_v / dielectric_strength_v_nm) * safety_factor
///
/// **Algorithm**:
/// 1. Convert voltage from millivolts to volts
/// 2. Convert dielectric strength from kV/mm to V/nm
/// 3. Calculate minimum clearance
/// 4. Apply safety factor (typically 2×)
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 100-200, Translation 1)
///
/// # Arguments
/// * `voltage_diff_mv` - Voltage difference in millivolts
/// * `dielectric_strength_kv_mm` - Dielectric strength in kV/mm
/// * `safety_factor` - Safety multiplier (typically 2)
///
/// # Returns
/// Minimum clearance in nanometers
///
/// # Examples
/// ```
/// use hwc_engine::constraint_manager::calculate_clearance_nm;
///
/// // 120V through Air (3 kV/mm) with 2× safety factor
/// let clearance = calculate_clearance_nm(120_000, 3.0, 2);
/// assert_eq!(clearance, 80_000);  // 0.08mm
///
/// // 120V through FR4 (20 kV/mm) with 2× safety factor
/// let clearance = calculate_clearance_nm(120_000, 20.0, 2);
/// assert_eq!(clearance, 12_000);  // 0.012mm
/// ```
pub fn calculate_clearance_nm(
    voltage_diff_mv: i64,
    dielectric_strength_kv_mm: f64,
    safety_factor: i64,
) -> i64 {
    // Convert voltage from millivolts to volts
    let voltage_v = voltage_diff_mv as f64 / 1000.0;

    // Convert dielectric strength from kV/mm to V/nm
    // kV/mm = 1000 V/mm = 1000 V / 1_000_000 nm = 0.001 V/nm
    let dielectric_v_nm = (dielectric_strength_kv_mm * 1000.0) / 1_000_000.0;

    // Calculate minimum clearance: voltage / dielectric_strength
    let min_clearance_nm = voltage_v / dielectric_v_nm;

    // Apply safety factor
    (min_clearance_nm * safety_factor as f64) as i64
}

/// Expand clearance zone around occupied voxels.
///
/// Creates a "forcefield" around a net by marking all voxels within the
/// clearance radius as forbidden for other nets.
///
/// **Algorithm**:
/// 1. For each occupied voxel
/// 2. Calculate clearance radius in voxels (ceiling division)
/// 3. Mark all voxels within Manhattan distance as clearance zone
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 100-200, Translation 1)
///
/// # Arguments
/// * `occupied_voxels` - Actual copper voxel locations
/// * `clearance_nm` - Clearance radius in nanometers
/// * `voxel_size_nm` - Size of one voxel in nanometers
///
/// # Returns
/// Vector of clearance zone voxels (forbidden space)
///
/// # Examples
/// ```
/// use hwc_engine::constraint_manager::expand_clearance_zone;
/// use hwc_engine::Point3D;
///
/// let occupied = vec![Point3D::new(0, 0, 0)];
/// let clearance_nm = 100_000;  // 0.1mm
/// let voxel_size_nm = 10_000;  // 0.01mm voxels
///
/// let zone = expand_clearance_zone(&occupied, clearance_nm, voxel_size_nm);
/// // Should create a sphere of ~10 voxel radius
/// assert!(zone.len() > 100);  // Many voxels in the zone
/// ```
pub fn expand_clearance_zone(
    occupied_voxels: &[Point3D],
    clearance_nm: i64,
    voxel_size_nm: i64,
) -> Vec<Point3D> {
    let mut clearance_voxels = Vec::new();

    // Calculate clearance radius in voxels (ceiling division)
    let clearance_voxels_radius = (clearance_nm + voxel_size_nm - 1) / voxel_size_nm;

    // For each occupied voxel, expand clearance zone
    for occupied in occupied_voxels {
        // Use 3D sphere approximation with Manhattan distance
        // Iterate through a cube and check Manhattan distance
        for dz in -clearance_voxels_radius..=clearance_voxels_radius {
            for dx in -clearance_voxels_radius..=clearance_voxels_radius {
                for dy in -clearance_voxels_radius..=clearance_voxels_radius {
                    let clearance_point = Point3D::new(
                        occupied.z + dz * voxel_size_nm,
                        occupied.x + dx * voxel_size_nm,
                        occupied.y + dy * voxel_size_nm,
                    );

                    // Check if within clearance radius using Manhattan distance
                    let distance = occupied.manhattan_distance(&clearance_point);
                    if distance <= clearance_nm {
                        clearance_voxels.push(clearance_point);
                    }
                }
            }
        }
    }

    // Remove duplicates (multiple occupied voxels may create overlapping zones)
    clearance_voxels.sort_by_key(|p| (p.z, p.x, p.y));
    clearance_voxels.dedup();

    clearance_voxels
}
