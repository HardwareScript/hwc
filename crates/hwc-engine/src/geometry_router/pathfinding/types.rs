//! Type definitions for pathfinding

use crate::constraint_manager::{ClearanceZone, LayerDirection, RouteConstraints};
use crate::geometry_router::stackup_slicing::RoutableMode;
use crate::netlist::NetId;
use crate::geometry_router::EntityGraph;

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

    /// v0.1.8: Routable mode for the current routing layer.
    /// The pathfinder queries this before placing trace segments.
    /// `None` defaults to full routing (backward compatible).
    pub layer_routable_mode: Option<RoutableMode>,

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
}
