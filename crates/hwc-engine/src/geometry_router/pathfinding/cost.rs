//! Cost calculation for pathfinding
//!
//! v0.1.8: All physical thresholds come from RouteConstraints (PDK profile).
//! All routing heuristic weights come from the profile's `routing:` block.
//! No hardcoded fallback values — the caller must provide valid constraints.

use crate::constraint_manager::{ClearanceZone, LayerDirection, RouteConstraints};
use crate::geometry::Point3D;
use crate::geometry_router::stackup_slicing::RoutableMode;
use crate::netlist::NetId;

/// Parameters for move cost calculation.
pub struct MoveCostParams<'a> {
    pub from: Point3D,
    pub to: Point3D,
    #[allow(dead_code)] // Populated by SDF router, consumed by future per-net clearance cost
    pub net_id: NetId,
    pub constraints: &'a RouteConstraints,
    #[allow(dead_code)] // Populated by SDF router, consumed by future clearance violation cost
    pub clearance_zones: &'a [ClearanceZone],
    pub layer_direction: Option<LayerDirection>,
    /// v0.1.7: Substrate layers for reference-plane void detection.
    /// When `is_high_speed_net` is true, crossing a void in the reference
    /// plane incurs an extreme penalty to force deviation.
    pub substrate_layers: Option<&'a [crate::geometry_router::substrate_types::SubstrateLayer]>,
    /// v0.1.7: Whether this net is classified as high-speed (≥1 GHz).
    /// High-speed nets incur SI penalties when crossing reference plane voids.
    pub is_high_speed_net: bool,

    // ── v0.1.8: Physical Synthesis Guardrails ──

    /// v0.1.9: Z-coordinate to RoutableMode mapping for dynamic per-node checking (Fix #2).
    /// Keys are layer Z-centers (in nm), values are routability modes.
    pub layer_routability_map: &'a rustc_hash::FxHashMap<i64, RoutableMode>,

    /// v0.1.8: Maximum length for `local_only` layers (in nanometers).
    /// If exceeded outside a component bounding box, the segment is rejected.
    pub max_local_route_length_nm: Option<i64>,

    /// v0.1.8: Current accumulated route length on local_only layers (nm).
    /// Reset to 0 at route start. Compared against `max_local_route_length_nm`.
    pub local_route_length_nm: i64,

    /// v0.1.8: Whether the current position is inside any component's bounding box.
    /// Used by local_only length-limit enforcement.
    pub is_inside_component: bool,

    /// v0.1.8: Via drill diameter (nm) for the Via-Portal Exemption.
    /// Read from profile's `via.min_diameter`. The portal tolerance is
    /// half this value.
    pub via_drill_diameter_nm: i64,

    /// v0.1.8: Net ID for pin co-location checks (Via-Portal Exemption).
    /// The exemption allows vertical via towers to penetrate component
    /// keep-out zones at the exact XY coordinate of an active pin on this net.
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

/// Calculate move cost for A* pathfinding with full clearance and crosstalk detection.
///
/// Applies penalties for vias, clearance violations, and routing constraints.
///
/// **Cost Structure (v0.1.8 - PDK-driven)**:
/// All cost weights come from the profile's `routing:` block.
/// No hardcoded values — the caller must provide valid constraints.
///
/// # Arguments
/// * `params` - MoveCostParams containing all routing context and heuristic weights
///
/// # Returns
/// Total movement cost
#[inline]
pub fn calculate_move_cost(params: &MoveCostParams) -> i64 {
    let mut cost = params.base_cost;

    let dx = params.to.x - params.from.x;
    let dy = params.to.y - params.from.y;
    let dz = params.to.z - params.from.z;

    if dz != 0 {
        cost += params.via_penalty;
    }

    if let Some(direction) = params.layer_direction {
        match direction {
            LayerDirection::EastWest => {
                if dy != 0 && dx == 0 {
                    cost += params.direction_penalty;
                }
            }
            LayerDirection::NorthSouth => {
                if dx != 0 && dy == 0 {
                    cost += params.direction_penalty;
                }
            }
            LayerDirection::Any => {}
        }
    }

    // Clearance violation detection is handled analytically in the
    // TopologicalRouter via ray-AABB intersection against the flat-packed geo-index.

    // v0.1.8: Apply penalties based on PDK constraint values.
    // Tight clearance penalty: when the net requires smaller clearance than typical
    // (indicates dense routing area or high-voltage net).
    if params.constraints.min_clearance_nm > 0 && params.constraints.min_clearance_nm < params.constraints.min_trace_width_nm * 2 {
        cost += params.tight_clearance_penalty;
    }

    // Crosstalk penalty: when the net has strict parallel length limits
    // (indicates high-speed or noise-sensitive net).
    if params.constraints.max_parallel_length_nm > 0 && params.constraints.max_parallel_length_nm < params.constraints.min_trace_width_nm * 10 {
        cost += params.crosstalk_penalty;
    }

    if params.constraints.impedance_ohm.is_some() {
        cost += params.impedance_penalty;
    }

    // =========================================================================
    // v0.1.7 Substrate & Reference-Plane Aware Routing
    // =========================================================================
    if params.is_high_speed_net {
        if let Some(substrate_layers) = params.substrate_layers {
            let has_void = substrate_layers.iter().any(|layer| {
                layer.layer_type == crate::geometry_router::substrate_types::SubstrateLayerType::Pour
                    && params.to.z >= layer.bbox.min.z
                    && params.to.z <= layer.bbox.max.z
                    && !layer.contains_nm(params.to.x, params.to.y, params.to.z)
            });

            if has_void {
                cost += params.reference_void_penalty;
            }
        }
    }

    // =========================================================================
    // v0.1.9: Physical Synthesis Guardrails (Dynamic Layer Checking - Fix #2)
    // =========================================================================

    // Guardrail 1: Non-Routable Layer (R25)
    // Query the target node's Z-coordinate against the routability map
    let target_layer_routable = params.layer_routability_map.get(&params.to.z).copied();

    // If the current layer has routable: false, reject with INFINITE cost.
    if let Some(RoutableMode::False) = target_layer_routable {
        return i64::MAX; // Hard block — trace cannot be placed on this layer
    }

    // Guardrail 1a: Local-Only Layer Length Limit
    // If the current layer has routable: local_only, enforce max length.
    if let Some(RoutableMode::LocalOnly) = target_layer_routable {
        if let Some(max_len) = params.max_local_route_length_nm {
            if params.local_route_length_nm > max_len && !params.is_inside_component {
                return i64::MAX; // Hard block — local route exceeded length limit
            }
        }
    }

    // Guardrail 2: Component Interior Lockout (Fix 3)
    // Mark component interiors as INFINITE cost on layers where the component
    // has physical material. Upper metal layers remain free for over-cell routing.
    if is_inside_component_keepout(params.to, params.component_keepouts) {
        // Guardrail 2a: Via-Portal Exemption — allow vertical via towers
        // to penetrate component keep-out zones at pin XY coordinates.
        if is_via_portal_exempt(params.to, params.via_drill_diameter_nm, params.active_net_pin_positions) {
            // Via-Portal Exemption granted — via can pass through
        } else {
            return i64::MAX; // Hard block — trace inside component interior
        }
    }

    cost
}



// =========================================================================
// v0.1.8 Physical Synthesis Guardrails — Helper Functions
// =========================================================================

/// v0.1.8: Check if a point is inside any component's layer-aware keep-out zone.
///
/// Keep-out zones are defined by (layer_z_min, layer_z_max, bbox_min_x, bbox_min_y,
/// bbox_max_x, bbox_max_y). A point is blocked only if it falls within a keep-out
/// zone that overlaps the point's Z-coordinate.
///
/// This is the core of the Interior Lockout rule: component bodies are blocked
/// on layers where they have physical material, but upper metal layers remain free.
#[inline]
fn is_inside_component_keepout(
    pos: Point3D,
    keepouts: &[(i64, i64, i64, i64, i64, i64)],
) -> bool {
    for &(z_min, z_max, min_x, min_y, max_x, max_y) in keepouts {
        if pos.z >= z_min && pos.z < z_max && pos.x >= min_x && pos.x <= max_x && pos.y >= min_y && pos.y <= max_y {
            return true;
        }
    }
    false
}

/// v0.1.8: Check if a point is exempt from component keep-out via the Via-Portal rule.
///
/// The Via-Portal Exemption allows vertical via towers to penetrate component
/// keep-out zones at the exact XY coordinate of an active pin belonging to
/// the current net. This prevents routing deadlocks where the router cannot
/// drop a via to reach a pin inside a component body.
///
/// # Tolerance
/// The exemption uses half the via drill diameter as the tolerance window.
/// This matches the physical via pad size in the PDK.
#[inline]
fn is_via_portal_exempt(
    pos: Point3D,
    via_drill_diameter_nm: i64,
    active_net_pin_positions: &[(i64, i64)],
) -> bool {
    if via_drill_diameter_nm <= 0 || active_net_pin_positions.is_empty() {
        return false;
    }
    let tolerance_nm = via_drill_diameter_nm / 2;
    for &(pin_x, pin_y) in active_net_pin_positions {
        if (pos.x - pin_x).abs() <= tolerance_nm && (pos.y - pin_y).abs() <= tolerance_nm {
            return true;
        }
    }
    false
}
