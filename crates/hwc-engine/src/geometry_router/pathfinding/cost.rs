//! Cost calculation for pathfinding

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

    /// v0.1.8: Routable mode for the current routing layer.
    /// The pathfinder queries this before placing trace segments.
    /// `None` defaults to full routing (backward compatible).
    pub layer_routable_mode: Option<RoutableMode>,

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
}

/// Calculate move cost for A* pathfinding with full clearance and crosstalk detection.
///
/// Applies penalties for vias, clearance violations, and routing constraints.
///
/// **Cost Structure (v0.1.5 - Task B1)**:
/// - Base cost: 1 (one physical-nm step)
/// - Via penalty: +50 (layer switch - vias are expensive and degrade signals)
/// - Preferred direction penalty: +10 (off-axis movement on Manhattan layers)
/// - Clearance violation penalty: +100 (moderate - avoid but don't block completely)
/// - Tight clearance penalty: +2 (for nets with strict clearance requirements)
/// - Crosstalk-sensitive penalty: +3 (for nets with strict parallel length limits)
/// - Impedance-controlled penalty: +1 (prefer direct paths for impedance control)
///
/// **v0.1.4 Full Implementation**:
/// Clearance violations are now HARD BLOCKS handled in
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
/// * `clearance_zones` - All clearance zones (for clearance violation detection)
/// * `layer_direction` - Preferred direction for the current layer (None = no preference)
///
/// # Returns
/// Total movement cost
#[inline]
pub fn calculate_move_cost(params: &MoveCostParams) -> i64 {
    let mut cost = 1i64;

    let dx = params.to.x - params.from.x;
    let dy = params.to.y - params.from.y;
    let dz = params.to.z - params.from.z;

    if dz != 0 {
        cost += 50;
    }

    if let Some(direction) = params.layer_direction {
        match direction {
            LayerDirection::EastWest => {
                if dy != 0 && dx == 0 {
                    cost += 10;
                }
            }
            LayerDirection::NorthSouth => {
                if dx != 0 && dy == 0 {
                    cost += 10;
                }
            }
            LayerDirection::Any => {}
        }
    }

    // Clearance violation detection is now handled analytically in the
    // TopologicalRouter via ray-AABB intersection against the flat-packed geo-index.
    // The legacy grid-based check_clearance_violation stub has been purged.

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
                layer.layer_type == crate::geometry_router::substrate_types::SubstrateLayerType::Pour
                    && params.to.z >= layer.bbox.min.z
                    && params.to.z <= layer.bbox.max.z
                    && !layer.contains_nm(params.to.x, params.to.y, params.to.z)
            });

            if has_void {
                cost += 5_000_000; // Extreme penalty to force deviation around dielectric voids
            }
        }
    }

    // =========================================================================
    // v0.1.8 Physical Synthesis Guardrails
    // =========================================================================

    // Guardrail 1: Non-Routable Layer (R25)
    // If the current layer has routable: false, reject with INFINITE cost.
    if let Some(RoutableMode::False) = params.layer_routable_mode {
        return i64::MAX; // Hard block — trace cannot be placed on this layer
    }

    // Guardrail 1a: Local-Only Layer Length Limit
    // If the current layer has routable: local_only, enforce max length.
    if let Some(RoutableMode::LocalOnly) = params.layer_routable_mode {
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
