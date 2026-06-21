//! Thermal gradient validation

use super::types::{NetProperties, PhysicsViolation};
use crate::bit_chunk::BitChunk;
use crate::geometry::Point3D;
use crate::geometry_router::substrate_types::NetId;
use crate::geometry_router::EntityGraph;

/// Validate thermal gradients in a chunk (detect thermal bottlenecks)
///
/// Detects "Heat Clusters" where traces are too narrow for their current load.
/// Uses cross-sectional analysis to find bottlenecks.
///
/// # Algorithm
/// 1. For each net with high current (> 500mA), calculate cross-section
/// 2. Calculate current density (mA/µm²)
/// 3. If density > 10 mA/µm², report thermal hotspot
/// 4. Use ampacity rule: 1A per 100µm² of copper (simplified)
///
/// # Arguments
/// * `grid` - The voxel grid
/// * `_bit_chunk` - BitChunk representation (unused, for future optimization)
/// * `base_x`, `base_y`, `base_z` - Base coordinates of the chunk
/// * `get_properties` - Function to retrieve net properties
///
/// # Returns
/// Vector of thermal hotspot violations
pub fn validate_thermal_gradients<F>(
    grid: &EntityGraph,
    _bit_chunk: &BitChunk,
    base_x: usize,
    base_y: usize,
    base_z: usize,
    get_properties: F,
) -> Vec<PhysicsViolation>
where
    F: Fn(NetId) -> NetProperties,
{
    let mut violations = Vec::new();
    let (size_x, size_y, size_z) = grid.size();

    // Track which nets we've already reported to avoid duplicates
    let mut reported_nets = rustc_hash::FxHashSet::default();

    // Check each voxel in the 4×4×4 chunk
    for lz in 0..4 {
        for ly in 0..4 {
            for lx in 0..4 {
                let x = base_x + lx;
                let y = base_y + ly;
                let z = base_z + lz;

                // Bounds check
                if x >= size_x || y >= size_y || z >= size_z {
                    continue;
                }

                if grid.is_empty(x, y, z) {
                    continue;
                }

                let net = grid.get_net(x, y, z);

                // Skip net 0 (substrate/background layer)
                if net == 0 {
                    continue;
                }

                let props = get_properties(net);
                let current_ma = props.current_density_ma_mm2;

                // Skip low-power signals (< 500mA)
                if current_ma < 500.0 {
                    continue;
                }

                // Skip if already reported
                if reported_nets.contains(&net) {
                    continue;
                }

                // Calculate cross-section in the Y-Z plane (for X-running traces)
                // This is a simplified model - real implementation would check all 3 planes
                let mut cross_section_count = 0;

                // Check 3×3 neighborhood in Y-Z plane
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let ny = (y as i32 + dy) as usize;
                        let nz = (z as i32 + dz) as usize;

                        if ny < size_y
                            && nz < size_z
                            && !grid.is_empty(x, ny, nz)
                            && grid.get_net(x, ny, nz) == net
                        {
                            cross_section_count += 1;
                        }
                    }
                }

                // Ampacity Rule: 1A per 100µm² of copper (simplified for v0.1.5)
                // Assuming 100µm × 100µm voxels = 10,000 µm² per voxel
                let voxel_area_um2 = 10_000.0; // 100µm × 100µm
                let total_area_um2 = cross_section_count as f64 * voxel_area_um2;
                let current_density_ma_um2 = current_ma / total_area_um2;

                // Threshold: 0.05 mA/µm² is a thermal hotspot
                // (This is 50 A/mm², which is the typical limit for PCB traces)
                // For a single 100µm × 100µm voxel (10,000 µm²), this allows 500mA
                if current_density_ma_um2 > 0.05 {
                    reported_nets.insert(net);

                    // Calculate temperature rise estimate
                    // Simplified: 10°C per mA/µm² above threshold
                    let temp_rise_c = (current_density_ma_um2 - 0.05) * 10.0;

                    let location = Point3D::new(
                        (x * 100_000) as i64, // Assuming 100µm voxel size
                        (y * 100_000) as i64,
                        (z * 1_000_000) as i64, // Assuming 1mm layer height
                    );

                    violations.push(PhysicsViolation::ThermalHotspot {
                        nets: vec![net],
                        location,
                        combined_power_mw: (current_ma * 3.3), // Assuming 3.3V for power estimate
                        temperature_rise_c: temp_rise_c,
                    });
                }
            }
        }
    }

    violations
}
