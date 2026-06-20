//! Collision detection and binary collision skip optimization

use crate::geometry::Point3D;
use crate::geometry_router::EntityGraph;

/// Binary Collision Skip: Check if all neighbors are valid using BitChunk.
///
/// NOTE: v0.1.8 - Stubbed out during VoxelGrid→EntityGraph migration.
/// The original implementation used VoxelGrid chunk collision masks.
/// Returns None to fall back to individual neighbor checking.
pub(super) fn try_binary_collision_skip(
    _current: Point3D,
    _neighbors: &[Point3D],
    _entity_graph: &EntityGraph,
    _voxel_size: crate::VoxelSize,
) -> Option<Vec<Point3D>> {
    None
}

/// Convert Point3D to voxel coordinates
#[inline]
pub(super) fn voxel_to_coords(
    point: Point3D,
    voxel_size: crate::VoxelSize,
) -> (usize, usize, usize) {
    let x = (point.x / voxel_size.x_nm.max(1)).max(0) as usize;
    let y = (point.y / voxel_size.y_nm.max(1)).max(0) as usize;
    let z = (point.z / voxel_size.z_nm.max(1)).max(0) as usize;
    (x, y, z)
}
