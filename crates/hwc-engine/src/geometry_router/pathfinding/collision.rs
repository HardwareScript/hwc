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
    _resolution_nm: i64,
) -> Option<Vec<Point3D>> {
    None
}

/// Convert Point3D to discrete coordinates based on resolution
#[inline]
pub(super) fn voxel_to_coords(
    point: Point3D,
    resolution_nm: i64,
) -> (usize, usize, usize) {
    let res = resolution_nm.max(1);
    let x = (point.x / res).max(0) as usize;
    let y = (point.y / res).max(0) as usize;
    let z = (point.z / res).max(0) as usize;
    (x, y, z)
}
