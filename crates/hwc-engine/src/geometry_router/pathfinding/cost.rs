//! Cost calculation for pathfinding

use crate::constraint_manager::{ClearanceZone, LayerDirection, RouteConstraints};
use crate::geometry::Point3D;
use crate::geometry_router::collision_detection::check_clearance_violation;
use crate::netlist::NetId;
use rustc_hash::FxHashSet;

/// Parameters for move cost calculation.
pub struct MoveCostParams<'a> {
    pub from: Point3D,
    pub to: Point3D,
    pub net_id: NetId,
    pub constraints: &'a RouteConstraints,
    pub voxel_size_nm: i64,
    pub occupied_voxels: &'a FxHashSet<Point3D>,
    pub clearance_zones: &'a [ClearanceZone],
    pub layer_direction: Option<LayerDirection>,
    /// v0.1.7: Substrate layers for reference-plane void detection.
    /// When `is_high_speed_net` is true, crossing a void in the reference
    /// plane incurs an extreme penalty to force deviation.
    pub substrate_layers: Option<&'a [crate::voxel_grid::SubstrateLayer]>,
    /// v0.1.7: Whether this net is classified as high-speed (≥1 GHz).
    /// High-speed nets incur SI penalties when crossing reference plane voids.
    pub is_high_speed_net: bool,
}

/// Calculate move cost for A* pathfinding with full clearance and crosstalk detection.
///
/// Applies penalties for vias, clearance violations, and crosstalk based on actual
/// voxel occupancy and clearance zones.
///
/// **Cost Structure (v0.1.5 - Task B1)**:
/// - Base cost: 1 (one voxel movement)
/// - Via penalty: +50 (layer switch - vias are expensive and degrade signals)
/// - Preferred direction penalty: +10 (off-axis movement on Manhattan layers)
/// - Clearance violation penalty: +100 (moderate - avoid but don't block completely)
/// - Crosstalk penalty: +50 (discourage parallel routing near other traces)
/// - Tight clearance penalty: +2 (for nets with strict clearance requirements)
/// - Crosstalk-sensitive penalty: +3 (for nets with strict parallel length limits)
/// - Impedance-controlled penalty: +1 (prefer direct paths for impedance control)
///
/// **v0.1.4 Full Implementation**:
/// Occupied voxels and clearance violations are now HARD BLOCKS handled in
/// route_net_deterministic with `continue` statements. This prevents A* cost
/// explosion where soft penalties cause the router to explore the entire board.
///
/// **v0.1.5 Task B1 - Via-Penalty & Preferred Direction**:
/// Added preferred direction enforcement per layer. Layers alternate between
/// horizontal (EastWest) and vertical (NorthSouth) to produce professional-looking
/// routes with minimal vias. Off-axis moves incur a +10 penalty.
///
/// # Arguments
/// * `from` - Starting position
/// * `to` - Destination position
/// * `net_id` - Net ID being routed
/// * `constraints` - Routing constraints for this net
/// * `voxel_size_nm` - Size of grid voxel (for spatial probing)
/// * `occupied_voxels` - Set of occupied points (for legacy compatibility with FxHashSet)
/// * `clearance_zones` - All clearance zones (for clearance violation detection)
/// * `layer_direction` - Preferred direction for the current layer (None = no preference)
///
/// # Returns
/// Total movement cost
#[inline]
pub fn calculate_move_cost(params: &MoveCostParams) -> i64 {
    let mut cost = 1i64; // Base cost: 1 voxel movement

    let dx = params.to.x - params.from.x;
    let dy = params.to.y - params.from.y;
    let dz = params.to.z - params.from.z;

    // Via penalty (layer change)
    // We use 50 instead of 10,000 to prevent "Heuristic Depression" where A*
    // explores the entire board to avoid the penalty. 50 is perfectly balanced
    // to discourage vias without stalling the algorithm.
    if dz != 0 {
        cost += 50;
    }

    // Preferred direction penalty (Task B1)
    // Penalize off-axis movement on Manhattan routing layers
    // This encourages horizontal traces on EastWest layers and vertical traces on NorthSouth layers
    if let Some(direction) = params.layer_direction {
        match direction {
            LayerDirection::EastWest => {
                // Prefer X-axis movement (horizontal)
                // Penalize Y-axis movement (vertical)
                if dy != 0 && dx == 0 {
                    cost += 10;
                }
            }
            LayerDirection::NorthSouth => {
                // Prefer Y-axis movement (vertical)
                // Penalize X-axis movement (horizontal)
                if dx != 0 && dy == 0 {
                    cost += 10;
                }
            }
            LayerDirection::Any => {
                // No preferred direction (power/ground planes)
            }
        }
    }

    // Clearance violation penalty (moderate)
    // We use a moderate penalty (not a hard block) because multiple routes from
    // the same pin need to pass through each other's clearance zones
    if check_clearance_violation(params.to, params.net_id, params.clearance_zones).is_some() {
        cost += 100; // Moderate penalty - avoid but don't block completely
    }

    // O(1) Crosstalk detection using Spatial Probing
    let crosstalk_penalty = calculate_crosstalk_penalty(
        params.from,
        params.to,
        params.voxel_size_nm,
        params.occupied_voxels,
    );
    cost += crosstalk_penalty;

    if params.constraints.min_clearance_nm < 200_000 {
        cost += 2;
    }

    if params.constraints.max_parallel_length_nm < 5_000_000 {
        cost += 3;
    }

    if params.constraints.impedance_ohm.is_some() {
        cost += 1;
    }

    // =========================================================================
    // v0.1.7 Substrate & Reference-Plane Aware Routing
    // =========================================================================
    // When routing a high-speed signal, crossing a split or void in the
    // ground/power reference plane causes signal reflections. Detect this
    // and apply an extreme penalty to force the router to deviate.
    if params.is_high_speed_net {
        if let Some(substrate_layers) = params.substrate_layers {
            // Look for a reference plane (Pour type) at the same Z as the target.
            // A point is "over a void" if it is within the pour's bounding box
            // but NOT contained by `contains_nm` (which excludes cutouts).
            let has_void = substrate_layers.iter().any(|layer| {
                layer.layer_type == crate::voxel_grid::SubstrateLayerType::Pour
                    && params.to.z >= layer.bbox.min.z
                    && params.to.z <= layer.bbox.max.z
                    && !layer.contains_nm(params.to.x, params.to.y, params.to.z)
            });

            if has_void {
                cost += 5_000_000; // Extreme penalty to force deviation around dielectric voids
            }
        }
    }

    cost
}

/// O(1) Spatial Probing for Crosstalk
/// Instead of iterating over all occupied arrays (O(N)), we directly generate the
/// coordinates 2mm to our left/right and query the FxHashSet (O(1)).
pub fn calculate_crosstalk_penalty(
    from: Point3D,
    to: Point3D,
    voxel_size_nm: i64,
    occupied_voxels: &FxHashSet<Point3D>,
) -> i64 {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let dz = to.z - from.z;

    // Only check for crosstalk on same layer (no Z change)
    if dz != 0 || (dx == 0 && dy == 0) {
        return 0;
    }

    let mut max_penalty = 0i64;

    // Probe up to 2mm away. Use max(1) to avoid division by zero
    let check_radius = (2_000_000_i64 / voxel_size_nm.max(1)).clamp(1, 10);

    if dx != 0 {
        // Moving Horizontally: Probe the Y coordinates above and below us
        for i in 1..=check_radius {
            let offset = i * voxel_size_nm;
            let p1 = Point3D::new(to.x, to.y + offset, to.z);
            let p2 = Point3D::new(to.x, to.y - offset, to.z);

            if occupied_voxels.contains(&p1) || occupied_voxels.contains(&p2) {
                let penalty = if offset < 500_000 {
                    50
                } else if offset < 1_000_000 {
                    30
                } else {
                    10
                };
                max_penalty = max_penalty.max(penalty);
                if max_penalty >= 50 {
                    break;
                }
            }
        }
    } else {
        // Moving Vertically: Probe the X coordinates left and right of us
        for i in 1..=check_radius {
            let offset = i * voxel_size_nm;
            let p1 = Point3D::new(to.x + offset, to.y, to.z);
            let p2 = Point3D::new(to.x - offset, to.y, to.z);

            if occupied_voxels.contains(&p1) || occupied_voxels.contains(&p2) {
                let penalty = if offset < 500_000 {
                    50
                } else if offset < 1_000_000 {
                    30
                } else {
                    10
                };
                max_penalty = max_penalty.max(penalty);
                if max_penalty >= 50 {
                    break;
                }
            }
        }
    }

    max_penalty
}
