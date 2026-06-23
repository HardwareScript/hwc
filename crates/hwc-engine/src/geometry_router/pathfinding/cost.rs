//! Cost calculation for pathfinding

use crate::constraint_manager::{ClearanceZone, LayerDirection, RouteConstraints};
use crate::geometry::Point3D;
use crate::geometry_router::collision_detection::check_clearance_violation;
use crate::geometry_router::stackup_slicing::RoutableMode;
use crate::netlist::NetId;

/// Parameters for move cost calculation.
pub struct MoveCostParams<'a> {
    pub from: Point3D,
    pub to: Point3D,
    pub net_id: NetId,
    pub constraints: &'a RouteConstraints,
    pub voxel_size_nm: i64,
    pub occupied_voxels: &'a rustc_hash::FxHashMap<Point3D, NetId>,
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
        params.net_id,
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

/// O(1) Spatial Probing for Crosstalk
/// Instead of iterating over all occupied arrays (O(N)), we directly generate the
/// coordinates 2mm to our left/right and query the FxHashSet (O(1)).
pub fn calculate_crosstalk_penalty(
    from: Point3D,
    to: Point3D,
    net_id: NetId,
    voxel_size_nm: i64,
    occupied_voxels: &rustc_hash::FxHashMap<Point3D, NetId>,
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

    let check_point = |p: Point3D| -> i64 {
        if let Some(&other_net_id) = occupied_voxels.get(&p) {
            // v0.1.7: Same-Net Repulsion (Node-to-Node Integrity)
            //
            // If the other net is the SAME as the current net, we apply a
            // moderate penalty (30) to discourage branching off existing traces.
            // This forces the router to prioritize reaching the PAD boundary
            // instead of taking a shortcut through a nearby segment of the same net.
            if other_net_id == net_id {
                return 30; // Discourage same-net branching
            }

            let dist_nm = ((p.x - to.x).pow(2) + (p.y - to.y).pow(2)) as f64;
            let offset = dist_nm.sqrt() as i64;

            if offset < 500_000 {
                50
            } else if offset < 1_000_000 {
                30
            } else {
                10
            }
        } else {
            0
        }
    };

    if dx != 0 {
        // Moving Horizontally: Probe the Y coordinates above and below us
        for i in 1..=check_radius {
            let offset = i * voxel_size_nm;
            let p1 = Point3D::new(to.x, to.y + offset, to.z);
            let p2 = Point3D::new(to.x, to.y - offset, to.z);

            max_penalty = max_penalty.max(check_point(p1)).max(check_point(p2));
            if max_penalty >= 50 {
                break;
            }
        }
    } else {
        // Moving Vertically: Probe the X coordinates left and right of us
        for i in 1..=check_radius {
            let offset = i * voxel_size_nm;
            let p1 = Point3D::new(to.x + offset, to.y, to.z);
            let p2 = Point3D::new(to.x - offset, to.y, to.z);

            max_penalty = max_penalty.max(check_point(p1)).max(check_point(p2));
            if max_penalty >= 50 {
                break;
            }
        }
    }

    max_penalty
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
