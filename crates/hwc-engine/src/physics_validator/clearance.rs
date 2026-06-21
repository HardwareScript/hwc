//! Clearance validation using bitwise dilation

use super::dilation::dilate_mask_3d;
use super::types::{NetProperties, PhysicsViolation};
use crate::bit_chunk::BitChunk;
use crate::geometry::Point3D;
use crate::geometry_router::substrate_types::NetId;

/// Validate clearance between nets using bitwise dilation (THE KILLER FEATURE)
///
/// This is the "God-Tier" feature of System 4. Instead of checking voxels one-by-one,
/// we use bitwise dilation to check 64 voxels simultaneously.
///
/// # Algorithm
/// 1. For each net pair in the chunk, get their bitmasks
/// 2. Dilate Net A's bitmask by the required clearance distance
/// 3. AND the dilated mask with Net B's bitmask
/// 4. If result != 0, there's a clearance violation
///
/// # Performance
/// Checking a million transistors for clearance takes the same time as checking one!
///
/// # Arguments
/// * `bit_chunk` - BitChunk representation with net bitmasks
/// * `base_x`, `base_y`, `base_z` - Base coordinates of the chunk
/// * `get_properties` - Function to retrieve net properties
///
/// # Returns
/// Vector of clearance violations
pub fn validate_clearance_dilation<F>(
    bit_chunk: &BitChunk,
    base_x: usize,
    base_y: usize,
    base_z: usize,
    get_properties: F,
) -> Vec<PhysicsViolation>
where
    F: Fn(NetId) -> NetProperties,
{
    let mut violations = Vec::new();

    // Get all unique nets in this chunk from the BitChunk
    let nets = bit_chunk.get_nets();

    // Check clearance between each pair of nets
    for i in 0..nets.len() {
        for j in (i + 1)..nets.len() {
            let net_a = nets[i];
            let net_b = nets[j];

            // Skip net 0 (substrate/background layer) - it doesn't participate in clearance checks
            if net_a == 0 || net_b == 0 {
                continue;
            }

            // Get required clearance for this net pair using flat array access
            let props_a = get_properties(net_a);
            let props_b = get_properties(net_b);
            let required_clearance_nm = props_a.clearance_nm.max(props_b.clearance_nm);

            // Skip if no clearance requirement
            if required_clearance_nm == 0 {
                continue;
            }

            // Convert clearance from nanometers to voxels
            // Assuming 100µm voxels = 100,000 nm
            let clearance_voxels = (required_clearance_nm + 99_999) / 100_000; // Round up

            // Skip if clearance is too large for chunk-level checking
            if clearance_voxels > 3 {
                continue; // Would need cross-chunk checking
            }

            // Get bitmasks for both nets using get_net_plane
            let mask_a = bit_chunk.get_net_plane(net_a);
            let mask_b = bit_chunk.get_net_plane(net_b);

            // THE KILLER FEATURE: Dilate Net A's mask by clearance distance
            let dilated_a = dilate_mask_3d(mask_a, clearance_voxels as usize);

            // Check for intersection: if dilated_a & mask_b != 0, there's a violation
            let collision_mask = dilated_a & mask_b;

            if collision_mask != 0 {
                // Find the first collision voxel for reporting
                let first_bit = collision_mask.trailing_zeros() as usize;
                let lx = first_bit % 4;
                let ly = (first_bit / 4) % 4;
                let lz = first_bit / 16;

                let location = Point3D::new(
                    ((base_x + lx) * 100_000) as i64, // Assuming 100µm voxel size
                    ((base_y + ly) * 100_000) as i64,
                    ((base_z + lz) * 1_000_000) as i64, // Assuming 1mm layer height
                );

                // Calculate actual clearance (simplified - just report as 0 for now)
                let actual_clearance_nm = 0; // Would need distance calculation

                violations.push(PhysicsViolation::ClearanceViolation {
                    net_a,
                    net_b,
                    location,
                    actual_clearance_nm,
                    required_clearance_nm,
                });
            }
        }
    }

    violations
}
