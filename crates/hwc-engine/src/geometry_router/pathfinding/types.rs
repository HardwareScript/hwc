//! Type definitions for pathfinding

use crate::constraint_manager::{ClearanceZone, LayerDirection, RouteConstraints};
use crate::geometry_router::stackup_slicing::RoutableMode;
use crate::geometry_router::EntityGraph;
use crate::netlist::NetId;

use compact_str::CompactString;

/// Parameters for deterministic routing
pub struct RoutingParams<'a> {
    pub net_id: NetId,
    pub constraints: &'a RouteConstraints,
    pub bounds: super::super::neighbor_generation::GridBounds,
    pub layer_direction: LayerDirection,
    pub resolution_nm: i64,
    pub clearance_zones: &'a [ClearanceZone],
    pub entity_graph: Option<&'a EntityGraph>,
    pub fixed_z_nm: Option<i64>,
    pub exempt_components: &'a [CompactString],
    pub substrate_layers: Option<&'a [crate::geometry_router::substrate_types::SubstrateLayer]>,
    pub is_high_speed_net: bool,

    // ── v0.1.8: Physical Synthesis Guardrails ──
    /// v0.1.9: Z-coordinate to RoutableMode mapping for dynamic per-node checking (Fix #2).
    /// Keys are layer Z-centers (in nm), values are routability modes.
    /// This allows the pathfinder to check routability dynamically as it explores different layers.
    /// Required for all routing operations.
    pub layer_routability_map: &'a rustc_hash::FxHashMap<i64, RoutableMode>,

    /// v0.1.8: Maximum length for `local_only` layers (in nanometers).
    /// If exceeded outside a component bounding box, the segment is rejected.
    pub max_local_route_length_nm: Option<i64>,

    /// v0.1.8: Via drill diameter (nm) for the Via-Portal Exemption.
    /// Read from profile's `via.min_diameter`. The portal tolerance is
    /// half this value.
    pub via_drill_diameter_nm: i64,

    /// v0.1.8: Net ID for pin co-location checks (Via-Portal Exemption).
    /// Pin XY coordinates for the current net.
    pub active_net_pin_positions: &'a [(i64, i64)],

    /// v0.1.8: Layer-aware component keep-out zones.
    /// Each entry is (layer_z_min, layer_z_max, bbox_min_x, bbox_min_y, bbox_max_x, bbox_max_y).
    /// Only layers where the component has material are blocked.
    pub component_keepouts: &'a [(i64, i64, i64, i64, i64, i64)],

    // ── v0.1.8: Routing Heuristic Weights (from PDK profile) ──
    /// Base cost for any single grid movement. Default: 1.
    pub base_cost: i64,
    /// Penalty for via transitions (layer changes). Default: 50.
    pub via_penalty: i64,
    /// Penalty for moving against preferred layer direction. Default: 10.
    pub direction_penalty: i64,
    /// Penalty when clearance is tight. Default: 2.
    pub tight_clearance_penalty: i64,
    /// Penalty for crosstalk risk. Default: 3.
    pub crosstalk_penalty: i64,
    /// Penalty for impedance-controlled nets. Default: 1.
    pub impedance_penalty: i64,
    /// Extreme penalty for crossing reference-plane voids. Default: 5_000_000.
    pub reference_void_penalty: i64,
}
