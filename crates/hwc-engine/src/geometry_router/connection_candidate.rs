//! Connection Candidate Optimization
//!
//! Pre-selects the best interface pairs before routing to reduce the
//! pathfinding search space. Separates topology optimization from pathfinding.
//!
//! Reference: `Docs/v0.1.9/Connection-Interface-Routing.md` §5

use crate::geometry::Point3D;
use crate::geometry_router::connection_interface::InterfaceId;
use crate::geometry_router::routing_intent::RoutingIntent;
use crate::netlist::NetId;

/// A potential connection between two interfaces.
///
/// Represents one candidate pair for routing. The router evaluates candidates
/// before pathfinding and selects the best interface pair to connect.
#[derive(Debug, Clone)]
pub struct ConnectionCandidate {
    /// Source interface
    pub source_interface: InterfaceId,
    /// Sink interface
    pub sink_interface: InterfaceId,
    /// Estimated cost (Euclidean distance + capability mismatch penalty)
    pub estimated_cost: i64,
    /// Whether this connection requires a via (layer change)
    pub requires_via: bool,
    /// Number of layer transitions
    pub layer_span: i64,
}

impl ConnectionCandidate {
    /// Create a new candidate with estimated cost.
    pub fn new(
        source_interface: InterfaceId,
        sink_interface: InterfaceId,
        source_pos: Point3D,
        sink_pos: Point3D,
    ) -> Self {
        // Euclidean distance as base cost
        let dx = (source_pos.x - sink_pos.x) as i128;
        let dy = (source_pos.y - sink_pos.y) as i128;
        let dz = (source_pos.z - sink_pos.z) as i128;
        let distance = crate::geometry_router::geometry_math::integer_sqrt(
            (dx * dx + dy * dy + dz * dz) as u128,
        ) as i64;

        let requires_via = source_pos.z != sink_pos.z;
        let layer_span = if requires_via { 1 } else { 0 };

        Self {
            source_interface,
            sink_interface,
            estimated_cost: distance,
            requires_via,
            layer_span,
        }
    }

    /// Heuristic scoring for candidate selection.
    ///
    /// Lower score = better candidate. The score combines:
    /// - Euclidean distance (base cost)
    /// - Via penalty (if layer change needed)
    /// - Critical path penalty (if on timing-critical net with layer span)
    pub fn score(&self, routing_intent: &RoutingIntent) -> i64 {
        let mut cost = self.estimated_cost;

        if self.requires_via {
            cost += 10_000;
        }

        if routing_intent.is_critical_path && self.layer_span > 1 {
            cost += 50_000; // Strong via penalty for timing-critical nets
        }

        cost
    }
}

/// Select the best interface pairs before routing.
///
/// For each terminal pair in the net, enumerates all interface combinations,
/// scores them, and returns the top N candidates.
///
/// # Arguments
/// * `net_id` - The net to find candidates for
/// * `source_interfaces` - Interfaces on the source component
/// * `sink_interfaces` - Interfaces on the sink component
/// * `routing_intent` - Routing intent for scoring
/// * `max_candidates` - Maximum number of candidates to return
pub fn select_connection_candidates(
    source_interfaces: &[(InterfaceId, Point3D)],
    sink_interfaces: &[(InterfaceId, Point3D)],
    routing_intent: &RoutingIntent,
    max_candidates: usize,
) -> Vec<ConnectionCandidate> {
    let mut candidates = Vec::new();

    for (src_id, src_pos) in source_interfaces {
        for (snk_id, snk_pos) in sink_interfaces {
            let candidate = ConnectionCandidate::new(*src_id, *snk_id, *src_pos, *snk_pos);
            candidates.push(candidate);
        }
    }

    // Sort by score (lower = better)
    candidates.sort_by_key(|c| c.score(routing_intent));
    candidates.truncate(max_candidates);
    candidates
}

/// Batch candidate selection for multiple nets.
///
/// Returns a map from net ID to its ranked candidates.
pub fn batch_select_candidates(
    net_sources: &[(
        NetId,
        Vec<(InterfaceId, Point3D)>,
        Vec<(InterfaceId, Point3D)>,
    )],
    routing_intent: &RoutingIntent,
    max_per_net: usize,
) -> rustc_hash::FxHashMap<NetId, Vec<ConnectionCandidate>> {
    let mut result = rustc_hash::FxHashMap::default();

    for (net_id, sources, sinks) in net_sources {
        let candidates = select_connection_candidates(sources, sinks, routing_intent, max_per_net);
        result.insert(*net_id, candidates);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry_router::connection_interface::InterfaceId;

    #[test]
    fn test_candidate_creation() {
        let src = InterfaceId::new(1);
        let snk = InterfaceId::new(2);
        let src_pos = Point3D::new(0, 0, 0);
        let snk_pos = Point3D::new(3000, 4000, 0);
        let candidate = ConnectionCandidate::new(src, snk, src_pos, snk_pos);

        assert_eq!(candidate.source_interface, src);
        assert_eq!(candidate.sink_interface, snk);
        assert_eq!(candidate.estimated_cost, 5000); // 3-4-5 triangle
        assert!(!candidate.requires_via);
    }

    #[test]
    fn test_candidate_requires_via() {
        let src = InterfaceId::new(1);
        let snk = InterfaceId::new(2);
        let src_pos = Point3D::new(0, 0, 0);
        let snk_pos = Point3D::new(0, 0, 1000);
        let candidate = ConnectionCandidate::new(src, snk, src_pos, snk_pos);

        assert!(candidate.requires_via);
        assert_eq!(candidate.layer_span, 1);
    }

    #[test]
    fn test_candidate_scoring() {
        let intent = RoutingIntent::default();
        let src = InterfaceId::new(1);
        let snk = InterfaceId::new(2);

        // Same layer, short distance
        let c1 =
            ConnectionCandidate::new(src, snk, Point3D::new(0, 0, 0), Point3D::new(1000, 0, 0));
        // Different layer, same distance
        let c2 =
            ConnectionCandidate::new(src, snk, Point3D::new(0, 0, 0), Point3D::new(1000, 0, 1000));

        assert!(c1.score(&intent) < c2.score(&intent));
    }

    #[test]
    fn test_select_candidates() {
        let src_interfaces = vec![
            (InterfaceId::new(1), Point3D::new(0, 0, 0)),
            (InterfaceId::new(2), Point3D::new(0, 1000, 0)),
        ];
        let snk_interfaces = vec![
            (InterfaceId::new(3), Point3D::new(10000, 0, 0)),
            (InterfaceId::new(4), Point3D::new(10000, 1000, 0)),
        ];
        let intent = RoutingIntent::default();

        let candidates = select_connection_candidates(&src_interfaces, &snk_interfaces, &intent, 2);
        assert!(candidates.len() <= 2);
        // Should be sorted by score
        if candidates.len() == 2 {
            assert!(candidates[0].score(&intent) <= candidates[1].score(&intent));
        }
    }
}
