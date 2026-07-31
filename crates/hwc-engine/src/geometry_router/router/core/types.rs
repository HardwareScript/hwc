//! Core types for the Geometry Router

use crate::constraint_manager::{ConstraintRulebook, LayerDirection};
use crate::geometry::Point3D;
use crate::geometry_router::bounding_box_tracker::BoundingBoxTracker;
use crate::geometry_router::neighbor_generation::GridBounds;
use crate::geometry_router::partition::PartitionGrid;
use crate::geometry_router::pathfinding::CostComposer;
use crate::geometry_router::query_engine::QueryStore;
use crate::geometry_router::routing_patterns::RoutingPattern;
use crate::geometry_router::substrate_types::SubstrateLayer;
use crate::geometry_router::types::{RoutingHeuristics, Via};
use crate::geometry_router::EntityGraph;
use rustc_hash::FxHashMap;

/// Request parameters for [`GeometryRouter::route_space`].
pub struct RouteSpaceRequest<'a> {
    pub grid_bbox: &'a crate::geometry::BoundingBox,
    pub nets: &'a FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>>,
    pub explicit_segments: Option<&'a [(crate::netlist::NetId, Vec<Point3D>)]>,
    pub obstacle_bboxes: &'a [crate::geometry::BoundingBox],
    pub substrate_layers: Option<&'a [SubstrateLayer]>,
    pub net_frequencies: &'a FxHashMap<crate::netlist::NetId, f64>,
    pub net_trace_widths: &'a FxHashMap<crate::netlist::NetId, i64>,
    /// v0.1.9: Per-net start/goal normals for perpendicular escape routing.
    pub net_normals: Option<&'a FxHashMap<crate::netlist::NetId, (crate::geometry_router::connection_interface::Normal2D, crate::geometry_router::connection_interface::Normal2D)>>,
    /// v0.1.9: Per-net escape stub distances in nanometers.
    pub net_escape_stubs: Option<&'a FxHashMap<crate::netlist::NetId, i64>>,
    /// v0.2.0: Per-net target Z-layer for explicit layer routing (layer: metal1).
    pub net_layer_targets: Option<&'a FxHashMap<crate::netlist::NetId, i64>>,
}

/// Copper pour definition for anti-pad generation.
#[derive(Debug, Clone)]
pub struct CopperPour {
    pub(crate) net_id: crate::netlist::NetId,
    /// Bottom Z elevation of the pour plane in nanometers.
    pub(crate) z_bottom_nm: i64,
}

/// Configuration for the Geometry Router.
/// Grouping these fields avoids passing many individual arguments.
#[derive(Clone)]
pub struct RouterConfig {
    pub area_threshold_nm2: i64,
    pub net_count_threshold: usize,
    pub is_manhattan: bool,
    pub profile_layers: Vec<String>,
    pub layer_z_positions: Vec<i64>,
    pub layer_materials: Vec<u8>,
    pub routing_heuristics: Option<RoutingHeuristics>,
    pub resolution_nm: i64,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            area_threshold_nm2: 1_000_000_000_000, // 1mm²
            net_count_threshold: 100,
            is_manhattan: false,
            profile_layers: Vec::new(),
            layer_z_positions: Vec::new(),
            layer_materials: Vec::new(),
            routing_heuristics: None,
            resolution_nm: 100, // Default resolution
        }
    }
}

/// Geometry Router: Main routing engine.
/// 
/// # Architecture Note (v0.2.0 Refactor)
/// GeometryRouter does NOT own EntityGraph - it is passed by mutable reference.
/// EntityGraph lives in Space and is the single source of truth for all routing data.
/// This ensures child routes synchronized from the routing database remain visible
/// during parent-level routing.
pub struct GeometryRouter {
    /// Grid bounds for routing
    pub(crate) bounds: GridBounds,

    /// Constraint rulebook (from Phase 1)
    pub(crate) constraints: ConstraintRulebook,

    /// Layer directions for Manhattan routing
    pub(crate) layer_directions: Vec<LayerDirection>,

    /// v0.1.8 continuous database snap-step resolution in nanometers
    pub(crate) resolution_nm: i64,

    /// Material registry for physical thickness lookups (v0.1.8)
    pub(crate) material_registry: crate::material::MaterialRegistry,

    /// All vias placed during routing
    pub(crate) vias: Vec<Via>,

    /// Copper pours (for anti-pad generation)
    pub(crate) copper_pours: Vec<CopperPour>,

    /// v0.1.7 Minkowski Integration: BoundingBoxTracker for obstacle inflation.
    pub(crate) bounding_box_tracker: BoundingBoxTracker,

    /// Configuration parameters
    pub(crate) config: RouterConfig,

    /// Substrate layers for reference-plane void detection.
    pub(crate) substrate_layers: Option<Vec<SubstrateLayer>>,

    /// Net frequencies in Hz.
    pub(crate) net_frequencies: FxHashMap<crate::netlist::NetId, f64>,

    /// Coarse partition grid for hierarchical G-Cell routing.
    pub(crate) partition_grid: Option<PartitionGrid>,

    /// v0.1.8 Salsa-style memoized query store.
    pub query_store: Option<QueryStore>,

    /// v0.1.8: Per-net routing pattern policies.
    pub route_net_policies: FxHashMap<crate::netlist::NetId, RoutingPattern>,

    /// Routing trace material ID resolved from the stackup.
    pub(crate) routing_material_id: u8,

    /// Trace width in nanometers resolved from fabrication constraints.
    pub(crate) trace_width_nm: i64,

    /// v0.1.9: Per-net trace widths in nanometers.
    pub(crate) net_trace_widths: FxHashMap<crate::netlist::NetId, i64>,

    /// v0.1.9: Per-net start/goal normals for perpendicular escape routing.
    pub(crate) net_normals: FxHashMap<crate::netlist::NetId, (crate::geometry_router::connection_interface::Normal2D, crate::geometry_router::connection_interface::Normal2D)>,

    /// v0.1.9: Per-net escape stub distances for perpendicular escape routing.
    pub(crate) net_escape_stubs: FxHashMap<crate::netlist::NetId, i64>,

    /// v0.1.9: Cost composer for intent-aware cost evaluation.
    pub(crate) cost_composer: CostComposer,

    /// v0.1.9: Per-net cost composers keyed by intent name.
    pub(crate) intent_composers: rustc_hash::FxHashMap<compact_str::CompactString, CostComposer>,

    /// v0.2.0: Per-net target Z-layer for explicit layer routing.
    pub(crate) net_layer_targets: FxHashMap<crate::netlist::NetId, i64>,
}
