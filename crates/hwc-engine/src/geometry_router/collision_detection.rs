//! Collision Detection and Clearance Enforcement
//!
//! This module handles clearance violation detection for routing.

use crate::constraint_manager::ClearanceZone;
use crate::geometry::Point3D;
use crate::netlist::NetId;

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
