//! Collision Detection and Clearance Enforcement
//!
//! This module handles voxel availability checking and clearance
//! violation detection for routing.

use crate::constraint_manager::ClearanceZone;
use crate::geometry::Point3D;
use crate::netlist::NetId;

/// Check if a voxel is available for routing.
///
/// A voxel is available if:
/// 1. It's not occupied by copper (empty)
/// 2. It's not in another net's clearance zone
/// 3. OR it's in the same net's clearance zone (nets can route through their own clearance)
///
/// **Algorithm**:
/// 1. Check if voxel is occupied
/// 2. Check if voxel is in any clearance zone
/// 3. If in clearance zone, check if it's the same net
///
/// # Arguments
/// * `point` - Point to check
/// * `net_id` - Net ID that wants to route through this point
/// * `clearance_zones` - All clearance zones in the design
///
/// # Returns
/// True if voxel is available, false otherwise
///
/// # Examples
/// ```
/// use hwc_engine::geometry_router::is_voxel_available;
/// use hwc_engine::{Point3D, netlist::NetId, constraint_manager::ClearanceZone};
///
/// let point = Point3D::new(0, 0, 0);
/// let net_id = NetId::new(0);
/// let clearance_zones = vec![];
///
/// // Empty voxel with no clearance zones
/// assert!(is_voxel_available(point, net_id, &clearance_zones));
/// ```
pub fn is_voxel_available(
    point: Point3D,
    net_id: NetId,
    clearance_zones: &[ClearanceZone],
) -> bool {
    // Check if point is in any clearance zone
    for zone in clearance_zones {
        // Skip if this is the same net (nets can route through their own clearance)
        if zone.net_id == net_id {
            continue;
        }

        // Check if point is in this zone's clearance voxels
        if zone.clearance_voxels.contains(&point) {
            return false; // Blocked by another net's clearance
        }
    }

    true // Available
}

/// Check for clearance violation at a point.
///
/// Returns the conflicting net ID if there's a violation, None otherwise.
///
/// # Arguments
/// * `point` - Point to check
/// * `net_id` - Net ID that wants to route through this point
/// * `clearance_zones` - All clearance zones in the design
///
/// # Returns
/// Some(conflicting_net_id) if violation, None if clear
///
/// # Examples
/// ```
/// use hwc_engine::geometry_router::check_clearance_violation;
/// use hwc_engine::{Point3D, netlist::NetId, constraint_manager::ClearanceZone};
///
/// let point = Point3D::new(0, 0, 0);
/// let net_id = NetId::new(0);
/// let clearance_zones = vec![];
///
/// // No violation
/// assert!(check_clearance_violation(point, net_id, &clearance_zones).is_none());
/// ```
pub fn check_clearance_violation(
    point: Point3D,
    net_id: NetId,
    clearance_zones: &[ClearanceZone],
) -> Option<NetId> {
    for zone in clearance_zones {
        // Skip if this is the same net
        if zone.net_id == net_id {
            continue;
        }

        // Check if point is in this zone's clearance voxels
        if zone.clearance_voxels.contains(&point) {
            return Some(zone.net_id); // Violation detected
        }
    }

    None // No violation
}

/// Mark a route as occupied in the grid.
///
/// Updates the VoxelGrid to mark all voxels along the path as occupied
/// by the specified net. Handles trace width by marking adjacent voxels.
///
/// # Arguments
/// * `voxel_grid` - Mutable reference to the voxel grid
/// * `path` - Routed path in nanometers
/// * `net_id` - Net ID
/// * `material` - Material ID (e.g., Copper)
/// * `voxel_size` - Voxel size for coordinate conversion
/// * `width_voxels` - Trace width in voxels (1 = single voxel width)
///
/// # Examples
/// ```
/// use hwc_engine::geometry_router::mark_route_occupied;
/// use hwc_engine::{Point3D, netlist::NetId, VoxelGrid, VoxelSize, test_utils::test_voxel_size};
///
/// let mut grid = VoxelGrid::new(10, 10, 2, test_voxel_size());
/// let voxel_size = VoxelSize { x_nm: 100_000, y_nm: 100_000, z_nm: 1_000_000 };
/// let path = vec![
///     Point3D::new(0, 0, 0),
///     Point3D::new(0, 100_000, 0),
/// ];
/// let net_id = NetId::new(0);
///
/// mark_route_occupied(&mut grid, &path, net_id, 1, &voxel_size, 1);
/// ```
pub fn mark_route_occupied(
    voxel_grid: &mut crate::voxel_grid::VoxelGrid,
    path: &[Point3D],
    net_id: NetId,
    material: u8,
    voxel_size: &crate::space::VoxelSize,
    width_voxels: usize,
) {
    use crate::voxel_grid::VoxelGrid;

    for point in path {
        // Convert point to voxel coordinates
        let (x, y, z) = VoxelGrid::nm_to_voxel(*point, voxel_size);

        // Mark the center voxel
        voxel_grid.set_occupied(x, y, z, material, crate::netlist::NetHandle::new(net_id.0));

        // Mark adjacent voxels for trace width
        if width_voxels > 1 {
            let radius = (width_voxels - 1) / 2;
            for dx in 0..=radius {
                for dy in 0..=radius {
                    if dx == 0 && dy == 0 {
                        continue; // Already marked center
                    }

                    // Mark in all 4 directions
                    if let Some(nx) = x.checked_add(dx) {
                        if let Some(ny) = y.checked_add(dy) {
                            voxel_grid.set_occupied(
                                nx,
                                ny,
                                z,
                                material,
                                crate::netlist::NetHandle::new(net_id.0),
                            );
                        }
                    }
                    if dx > 0 {
                        if let Some(nx) = x.checked_sub(dx) {
                            if let Some(ny) = y.checked_add(dy) {
                                voxel_grid.set_occupied(
                                    nx,
                                    ny,
                                    z,
                                    material,
                                    crate::netlist::NetHandle::new(net_id.0),
                                );
                            }
                        }
                    }
                    if dy > 0 {
                        if let Some(nx) = x.checked_add(dx) {
                            if let Some(ny) = y.checked_sub(dy) {
                                voxel_grid.set_occupied(
                                    nx,
                                    ny,
                                    z,
                                    material,
                                    crate::netlist::NetHandle::new(net_id.0),
                                );
                            }
                        }
                    }
                    if dx > 0 && dy > 0 {
                        if let Some(nx) = x.checked_sub(dx) {
                            if let Some(ny) = y.checked_sub(dy) {
                                voxel_grid.set_occupied(
                                    nx,
                                    ny,
                                    z,
                                    material,
                                    crate::netlist::NetHandle::new(net_id.0),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
