//! Test utilities shared across all test modules

use crate::geometry_router::sdf_generator::VoxelSize;

/// Helper function to create a standard test VoxelSize
pub fn test_voxel_size() -> VoxelSize {
    VoxelSize {
        x_nm: 100_000,
        y_nm: 100_000,
        z_nm: 1_000_000,
    }
}
