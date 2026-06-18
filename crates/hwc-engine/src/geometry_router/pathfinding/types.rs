//! Type definitions for pathfinding

use crate::constraint_manager::{ClearanceZone, LayerDirection, RouteConstraints};
use crate::geometry::Point3D;
use crate::netlist::NetId;
use crate::voxel_grid::VoxelGrid;
use rustc_hash::FxHashSet;

use super::super::coarse_grid::CoarseNode;

use compact_str::CompactString;

/// Parameters for deterministic routing
pub struct RoutingParams<'a> {
    pub net_id: NetId,
    pub constraints: &'a RouteConstraints,
    pub bounds: super::super::neighbor_generation::GridBounds,
    pub layer_direction: LayerDirection,
    pub voxel_size: crate::VoxelSize,
    pub clearance_zones: &'a [ClearanceZone],
    pub occupied_voxels: &'a rustc_hash::FxHashMap<Point3D, NetId>,
    pub voxel_grid: Option<&'a VoxelGrid>, // Optional: for Binary Collision Skip
    pub corridor: Option<&'a FxHashSet<CoarseNode>>, // Optional: for Hierarchical Corridor Search
    /// v0.1.7: Fixed Z-height for Planar Lock (2.5D Routing)
    pub fixed_z_nm: Option<i64>,
    /// v0.1.7: Components exempt from collision (for Escape Exemption)
    pub exempt_components: &'a [CompactString],
    /// v0.1.7: Substrate layers for reference-plane void detection in high-speed routing.
    pub substrate_layers: Option<&'a [crate::voxel_grid::SubstrateLayer]>,
    /// v0.1.7: Whether this net is classified as high-speed (≥1 GHz).
    pub is_high_speed_net: bool,
}

/// A* node for priority queue.
///
/// Ordered by f-score (g + h), with coordinate-strict tie-breaking for Git stability.
///
/// **Tie-Breaking Order**: Cost -> Z -> X -> Y
/// This ensures that if two paths have equal cost, the compiler always chooses
/// the same one, making builds reproducible across Git commits.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct AStarNode {
    pub(super) position: Point3D,
    pub(super) f_score: i64, // g + h
}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Min-heap: lower f-score is higher priority
        // Tie-breaking: Cost -> Z -> X -> Y (coordinate-strict for determinism)
        // For equal f-scores, we want LOWER coordinates to have HIGHER priority
        // So we compare other to self (reversed) for coordinates
        other
            .f_score
            .cmp(&self.f_score)
            .then_with(|| other.position.z.cmp(&self.position.z))
            .then_with(|| other.position.x.cmp(&self.position.x))
            .then_with(|| other.position.y.cmp(&self.position.y))
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
