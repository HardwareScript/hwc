//! Bitwise dilation utilities for clearance checking

/// Dilate a bitmask by N voxels (for clearance checking)
///
/// This is the "killer feature" of bit-parallel physics validation.
/// To check if Net A violates Net B's clearance, we:
/// 1. Dilate Net B's bitmask by the clearance distance
/// 2. AND it with Net A's bitmask
/// 3. If result != 0, there's a violation
///
/// Total time: Microseconds (not milliseconds!)
///
/// # Arguments
/// * `mask` - Original bitmask
/// * `distance` - Dilation distance in voxels
///
/// # Returns
/// Dilated bitmask
///
/// # Example
/// ```
/// # use hwc_engine::physics_validator::dilate_mask_1d;
/// // Net occupies voxel 0
/// let net_mask = 0b1;
///
/// // Dilate by 1 voxel (clearance zone)
/// let dilated = dilate_mask_1d(net_mask, 1);
///
/// // Now voxels 0 and 1 are in the clearance zone
/// assert_eq!(dilated, 0b11);
/// ```
pub fn dilate_mask_1d(mask: u64, distance: usize) -> u64 {
    let mut result = mask;

    for _ in 0..distance {
        // Shift left and right, then OR with original
        result |= result << 1;
        result |= result >> 1;
    }

    result
}

/// Dilate a bitmask in 3D (for full clearance checking)
///
/// This dilates in X, Y, and Z directions simultaneously.
/// For a 4×4×4 chunk, we need to handle wrapping carefully.
///
/// # Arguments
/// * `mask` - Original bitmask (64 bits for 4×4×4 chunk)
/// * `distance` - Dilation distance in voxels
///
/// # Returns
/// Dilated bitmask
pub fn dilate_mask_3d(mask: u64, distance: usize) -> u64 {
    let mut result = mask;

    for _ in 0..distance {
        let mut next = result;

        // Dilate in X direction (within each 4-voxel row)
        for z in 0..4 {
            for y in 0..4 {
                let row_start = z * 16 + y * 4;
                let row_mask = (result >> row_start) & 0b1111;

                let dilated_row = row_mask | (row_mask << 1) | (row_mask >> 1);
                let dilated_row = dilated_row & 0b1111; // Clamp to 4 bits

                // Clear old row and set new row
                next &= !(0b1111u64 << row_start);
                next |= dilated_row << row_start;
            }
        }

        // Dilate in Y direction (between rows in same Z layer)
        for z in 0..4 {
            for y in 0..3 {
                let row_start = z * 16 + y * 4;
                let next_row_start = row_start + 4;

                let row = (next >> row_start) & 0b1111;
                let next_row = (next >> next_row_start) & 0b1111;

                // OR adjacent rows
                next |= row << next_row_start;
                next |= next_row << row_start;
            }
        }

        // Dilate in Z direction (between layers)
        for z in 0..3 {
            let layer_start = z * 16;
            let next_layer_start = (z + 1) * 16;

            let layer = (next >> layer_start) & 0xFFFF;
            let next_layer = (next >> next_layer_start) & 0xFFFF;

            // OR adjacent layers
            next |= layer << next_layer_start;
            next |= next_layer << layer_start;
        }

        result = next;
    }

    result
}
