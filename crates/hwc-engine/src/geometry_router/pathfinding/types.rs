//! Type definitions for pathfinding

use crate::constraint_manager::{ClearanceZone, LayerDirection, RouteConstraints};
use crate::geometry::Point3D;
use crate::netlist::NetId;
use crate::geometry_router::EntityGraph;
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
    pub entity_graph: Option<&'a EntityGraph>,
    pub corridor: Option<&'a FxHashSet<CoarseNode>>,
    pub fixed_z_nm: Option<i64>,
    pub exempt_components: &'a [CompactString],
    pub substrate_layers: Option<&'a [crate::voxel_grid::SubstrateLayer]>,
    pub is_high_speed_net: bool,
}
