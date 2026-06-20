use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::route_decomposition::RouteSegment;
use crate::netlist::NetId;
use rustc_hash::FxHashMap;

/// Per-net routing state maintained during the routing pass.
#[derive(Clone, Debug)]
pub struct NetRouteState {
    pub net_id: NetId,
    pub constraints: crate::constraint_manager::RouteConstraints,
    /// Route segments assigned to this net
    pub segments: Vec<RouteSegment>,
    /// Routed waypoints per segment
    pub routed_paths: FxHashMap<usize, Vec<Point3D>>,
    /// Whether this net has been fully routed
    pub is_complete: bool,
    /// Number of attempts made for this net
    pub attempts: usize,
}

/// Configuration for net ordering during routing.
#[derive(Clone, Debug)]
pub struct NetRoutingOrder {
    pub net_id: NetId,
    /// Priority score (lower = route first)
    /// Based on: bounding box area, pin count, net criticality
    pub priority: i64,
    /// Bounding box area (for ordering)
    pub bbox_area: i64,
}

/// The Multi-Net Routing Manager.
///
/// Isolates routing identities and parameters of separate nets.
/// Manages net ordering, constraint propagation, and same-net collision bypass.
pub struct MultiNetManager {
    /// Per-net routing states
    pub states: FxHashMap<NetId, NetRouteState>,
    /// Ordered list of nets to route (by priority)
    pub routing_order: Vec<NetRoutingOrder>,
    /// Global clearance rules between different nets
    pub inter_net_clearance_nm: i64,
}

impl MultiNetManager {
    pub fn new(inter_net_clearance_nm: i64) -> Self {
        Self {
            states: FxHashMap::default(),
            routing_order: Vec::new(),
            inter_net_clearance_nm,
        }
    }

    /// Initialize routing state for a net from its decomposition.
    pub fn register_net(
        &mut self,
        net_id: NetId,
        constraints: crate::constraint_manager::RouteConstraints,
        segments: Vec<RouteSegment>,
    ) {
        let state = NetRouteState {
            net_id,
            constraints,
            segments,
            routed_paths: FxHashMap::default(),
            is_complete: false,
            attempts: 0,
        };
        self.states.insert(net_id, state);
    }

    /// Compute routing priority for all registered nets.
    /// Nets with smaller bounding box area route first (less flexibility).
    /// Nets with more pins get higher priority (more connections).
    pub fn compute_routing_order(&mut self) {
        self.routing_order.clear();
        for (&net_id, state) in &self.states {
            let area: i64 = state
                .segments
                .iter()
                .map(|s| {
                    let dx = (s.from_pin.position.x - s.to_pin.position.x).abs();
                    let dy = (s.from_pin.position.y - s.to_pin.position.y).abs();
                    dx * dy
                })
                .sum();
            let pin_count = state.segments.len() as i64;
            // Smaller area + more pins = higher priority (lower score)
            let priority = area / (pin_count + 1);
            self.routing_order.push(NetRoutingOrder {
                net_id,
                priority,
                bbox_area: area,
            });
        }
        self.routing_order.sort_by_key(|o| o.priority);
    }

    /// Check if a point conflicts with a different-net obstacle.
    /// Allows same-net traces to overlap (same-net bypass).
    #[inline]
    pub fn check_conflict(
        &self,
        point: Point3D,
        net_id: NetId,
        occupied: &FxHashMap<Point3D, NetId>,
    ) -> bool {
        if let Some(&occupant_net) = occupied.get(&point) {
            if occupant_net == net_id {
                return false; // Same net — allowed to overlap
            }
            return true; // Different net — conflict
        }
        false // Empty — no conflict
    }

    /// Record that a net occupies a position.
    #[inline]
    pub fn occupy_position(
        &self,
        occupied: &mut FxHashMap<Point3D, NetId>,
        pos: Point3D,
        net_id: NetId,
    ) {
        occupied.insert(pos, net_id);
    }

    /// Mark a net's segment as routed.
    pub fn mark_segment_routed(
        &mut self,
        net_id: NetId,
        segment_id: usize,
        waypoints: Vec<Point3D>,
    ) {
        if let Some(state) = self.states.get_mut(&net_id) {
            state.routed_paths.insert(segment_id, waypoints);
            let total_segments = state.segments.len();
            let routed_count = state.routed_paths.len();
            state.is_complete = routed_count >= total_segments;
        }
    }

    /// Check if all nets are fully routed.
    pub fn all_complete(&self) -> bool {
        self.states.values().all(|s| s.is_complete)
    }

    /// Get statistics about the routing session.
    pub fn stats(&self) -> MultiNetStats {
        let total = self.states.len();
        let complete = self.states.values().filter(|s| s.is_complete).count();
        let total_segments: usize = self.states.values().map(|s| s.segments.len()).sum();
        let routed_segments: usize = self.states.values().map(|s| s.routed_paths.len()).sum();
        MultiNetStats {
            total_nets: total,
            completed_nets: complete,
            total_segments,
            routed_segments,
        }
    }

    /// Compute the bounding box spanning all pins of a net.
    pub fn net_bounding_box(&self, net_id: NetId) -> Option<BoundingBox> {
        let state = self.states.get(&net_id)?;
        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;

        for seg in &state.segments {
            let positions = [&seg.from_pin.position, &seg.to_pin.position];
            for pos in positions {
                min_x = min_x.min(pos.x);
                min_y = min_y.min(pos.y);
                max_x = max_x.max(pos.x);
                max_y = max_y.max(pos.y);
            }
        }

        if min_x == i64::MAX {
            return None;
        }

        Some(BoundingBox {
            min: Point3D::new(min_x, min_y, 0),
            max: Point3D::new(max_x, max_y, 0),
        })
    }

    /// Increment attempt counter for a net.
    pub fn record_attempt(&mut self, net_id: NetId) {
        if let Some(state) = self.states.get_mut(&net_id) {
            state.attempts += 1;
        }
    }
}

impl Default for MultiNetManager {
    fn default() -> Self {
        Self::new(200_000) // 0.2mm default inter-net clearance
    }
}

/// Statistics about a multi-net routing session.
#[derive(Clone, Debug)]
pub struct MultiNetStats {
    pub total_nets: usize,
    pub completed_nets: usize,
    pub total_segments: usize,
    pub routed_segments: usize,
}
