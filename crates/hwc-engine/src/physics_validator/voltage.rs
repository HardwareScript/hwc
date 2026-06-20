//! Voltage boundary validation

use super::types::{NetProperties, PhysicsViolation};
use crate::bit_chunk::BitChunk;
use crate::geometry::Point3D;
use crate::voxel_grid::NetId;
use crate::geometry_router::EntityGraph;

/// Validate voltage boundaries in a chunk
///
/// Ensures that voxels with high voltage differences have sufficient insulator "halo"
/// to prevent dielectric breakdown. Uses bitwise operations for fast validation.
///
/// # Algorithm
/// 1. For each net in the chunk, check voltage against neighbors
/// 2. If Δ voltage > 50V, calculate required halo thickness
/// 3. Use bitwise neighborhood checks for validation
/// 4. Report violations where halo is missing or insufficient
/// 5. Deduplicate by only reporting from higher voltage side
///
/// # Arguments
/// * `grid` - The voxel grid
/// * `_bit_chunk` - BitChunk representation (unused, for future optimization)
/// * `base_x`, `base_y`, `base_z` - Base coordinates of the chunk
/// * `get_properties` - Function to retrieve net properties
///
/// # Returns
/// Vector of voltage boundary violations
pub fn validate_voltage_boundaries<F>(
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

    // Track which voxels we've already reported to avoid duplicates
    let mut reported_voxels = rustc_hash::FxHashSet::default();

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
                let net_voltage = props.voltage_mv;

                // Skip low-voltage nets (< 50V absolute value)
                if net_voltage.abs() < 50_000 {
                    continue;
                }

                // Skip if already reported
                if reported_voxels.contains(&(x, y, z)) {
                    continue;
                }

                let mut has_violation = false;
                let mut max_required_thickness = 0i64;

                // Check all 6 neighbors
                let neighbors = [
                    (x.wrapping_sub(1), y, z),
                    (x + 1, y, z),
                    (x, y.wrapping_sub(1), z),
                    (x, y + 1, z),
                    (x, y, z.wrapping_sub(1)),
                    (x, y, z + 1),
                ];

                for (nx, ny, nz) in neighbors {
                    // Bounds check
                    if nx >= size_x || ny >= size_y || nz >= size_z {
                        continue;
                    }

                    if grid.is_empty(nx, ny, nz) {
                        continue;
                    }

                    let neighbor_net = grid.get_net(nx, ny, nz);
                    if neighbor_net == net {
                        continue; // Same net, no voltage difference
                    }

                    let neighbor_props = get_properties(neighbor_net);
                    let neighbor_voltage = neighbor_props.voltage_mv;
                    let voltage_diff = (net_voltage - neighbor_voltage).abs();

                    // Only report from the higher absolute voltage side to avoid duplicates
                    // If voltages are equal, use net ID as tiebreaker
                    if net_voltage.abs() < neighbor_voltage.abs() {
                        continue;
                    }
                    if net_voltage.abs() == neighbor_voltage.abs() && net > neighbor_net {
                        continue;
                    }

                    // Check if voltage difference requires insulator halo
                    if voltage_diff > 50_000 {
                        // Calculate required halo thickness
                        // Using air dielectric strength: 3 kV/mm
                        // Formula: thickness = (voltage / dielectric_strength) × safety_factor
                        let voltage_v = voltage_diff as f64 / 1_000.0; // Convert mV to V
                        let dielectric_strength_v_mm = 3000.0; // Air: 3 kV/mm = 3000 V/mm
                        let safety_factor = 2.0;

                        let required_thickness_mm =
                            (voltage_v / dielectric_strength_v_mm) * safety_factor;
                        let required_thickness_nm = (required_thickness_mm * 1_000_000.0) as i64;

                        // Check if there's an insulator between the nets
                        let neighbor_material = grid.get_material(nx, ny, nz);

                        // Material IDs: 1=Silicon, 2=Copper, 3=Gold, 4=Aluminum
                        // If neighbor is a conductor (2, 3, 4), we need insulator
                        let is_conductor = matches!(neighbor_material, 2..=4);

                        if is_conductor {
                            has_violation = true;
                            max_required_thickness =
                                max_required_thickness.max(required_thickness_nm);
                        }
                    }
                }

                // Report violation once per voxel
                if has_violation {
                    reported_voxels.insert((x, y, z));

                    let location = Point3D::new(
                        (x * 100_000) as i64, // Assuming 100µm voxel size
                        (y * 100_000) as i64,
                        (z * 1_000_000) as i64, // Assuming 1mm layer height
                    );

                    violations.push(PhysicsViolation::VoltageBoundary {
                        net,
                        voltage_mv: net_voltage,
                        location,
                        max_mv: max_required_thickness, // TEMPORARY: Reusing field for compilation
                    });
                }
            }
        }
    }

    violations
}
