use std::collections::{HashMap, HashSet, VecDeque};

use crate::geometry::Point3D;
use crate::geometry_router::route_decomposition::VirtualJunction;
use crate::geometry_router::spatial_index::IndexedSegment;

/// Maximum distance (nm) between two endpoints to consider them snapped together.
const SNAP_DISTANCE_NM: i64 = 500;

/// Adjacency list mapping net_id → list of connected node IDs (segment endpoints).
/// Each node represents a unique endpoint position quantized to snap grid.
#[derive(Clone, Debug)]
pub struct ConnectivityGraph {
    /// net_id → set of node IDs belonging to that net
    pub net_nodes: HashMap<u32, HashSet<u32>>,
    /// node_id → set of neighbor node_ids (same-net adjacency)
    pub adjacency: HashMap<u32, HashSet<u32>>,
    /// node_id → position (for violation reporting)
    pub node_positions: HashMap<u32, Point3D>,
    /// node_id → net_id (reverse lookup)
    pub node_nets: HashMap<u32, u32>,
}

/// Violation detected during connectivity verification.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectivityViolation {
    /// A pin is not reachable from any other pin on its net.
    DisconnectedPin {
        net_id: u32,
        pin_id: u32,
        location: (i64, i64),
    },
    /// Two different nets are electrically connected (short) at a point.
    UnwaivedShort {
        net_a: u32,
        net_b: u32,
        location: (i64, i64),
    },
    /// An entire net has no connected segments or only isolated fragments.
    BrokenNet { net_id: u32 },
}

/// Result of full connectivity verification.
#[derive(Clone, Debug)]
pub struct ConnectivityResult {
    pub violations: Vec<ConnectivityViolation>,
    pub nets_checked: u32,
    pub pins_verified: u32,
    pub time_ms: u64,
}

/// Check if two points are within snap distance.
#[inline]
fn points_snapped(a: &Point3D, b: &Point3D) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    let dist_sq = dx * dx + dy * dy + dz * dz;
    dist_sq <= SNAP_DISTANCE_NM * SNAP_DISTANCE_NM
}

/// Assign a unique node ID to a point, scoped by net.
/// Same-position endpoints on the same net merge; same-position on different nets
/// remain distinct so short detection works correctly.
#[inline]
fn quantize_node_id(
    pos: Point3D,
    net_id: u32,
    node_map: &mut HashMap<(i64, i64, i64, u32), u32>,
    next_id: &mut u32,
) -> u32 {
    let key = (pos.x, pos.y, pos.z, net_id);
    if let Some(&id) = node_map.get(&key) {
        return id;
    }
    let id = *next_id;
    *next_id = next_id.checked_add(1).unwrap_or(0);
    node_map.insert(key, id);
    id
}

/// Build connectivity graph from routed segments and virtual junctions.
///
/// Each segment endpoint becomes a node. Two nodes on the same net are adjacent
/// if they share a segment or are within `SNAP_DISTANCE_NM` of each other.
pub fn build_connectivity_graph(
    segments: &[IndexedSegment],
    junctions: &[VirtualJunction],
) -> ConnectivityGraph {
    let mut adjacency: HashMap<u32, HashSet<u32>> = HashMap::new();
    let mut net_nodes: HashMap<u32, HashSet<u32>> = HashMap::new();
    let mut node_positions: HashMap<u32, Point3D> = HashMap::new();
    let mut node_nets: HashMap<u32, u32> = HashMap::new();
    let mut node_map: HashMap<(i64, i64, i64, u32), u32> = HashMap::new();
    let mut next_id: u32 = 0;

    for seg in segments {
        let net = seg.net_id as u32;
        let start_id = quantize_node_id(seg.start, net, &mut node_map, &mut next_id);
        let end_id = quantize_node_id(seg.end, net, &mut node_map, &mut next_id);

        node_positions.insert(start_id, seg.start);
        node_positions.insert(end_id, seg.end);
        node_nets.insert(start_id, net);
        node_nets.insert(end_id, net);

        net_nodes.entry(net).or_default().insert(start_id);
        net_nodes.entry(net).or_default().insert(end_id);

        adjacency.entry(start_id).or_default().insert(end_id);
        adjacency.entry(end_id).or_default().insert(start_id);
    }

    for junc in junctions {
        let net = junc.net_id.raw();
        let junc_id = quantize_node_id(junc.position, net, &mut node_map, &mut next_id);

        node_positions.insert(junc_id, junc.position);
        node_nets.insert(junc_id, net);
        net_nodes.entry(net).or_default().insert(junc_id);

        for &seg_id in &junc.connected_segments {
            if let Some(seg) = segments.get(seg_id) {
                if seg.net_id as u32 == net {
                    let start_id = quantize_node_id(seg.start, net, &mut node_map, &mut next_id);
                    let end_id = quantize_node_id(seg.end, net, &mut node_map, &mut next_id);
                    adjacency.entry(junc_id).or_default().insert(start_id);
                    adjacency.entry(start_id).or_default().insert(junc_id);
                    adjacency.entry(junc_id).or_default().insert(end_id);
                    adjacency.entry(end_id).or_default().insert(junc_id);
                }
            }
        }
    }

    ConnectivityGraph {
        net_nodes,
        adjacency,
        node_positions,
        node_nets,
    }
}

/// Check reachability within each net using BFS.
/// Returns violations for disconnected pins, broken nets, and shorts.
pub fn check_reachability(graph: &ConnectivityGraph) -> Vec<ConnectivityViolation> {
    let mut violations = Vec::new();

    for (&net_id, nodes) in &graph.net_nodes {
        if nodes.is_empty() {
            violations.push(ConnectivityViolation::BrokenNet { net_id });
            continue;
        }

        let mut visited: HashSet<u32> = HashSet::new();
        let mut queue: VecDeque<u32> = VecDeque::new();

        if let Some(&start) = nodes.iter().next() {
            queue.push_back(start);
            visited.insert(start);
        }

        while let Some(node) = queue.pop_front() {
            if let Some(neighbors) = graph.adjacency.get(&node) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor)
                        && graph.node_nets.get(&neighbor) == Some(&net_id)
                    {
                        visited.insert(neighbor);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        for &node in nodes {
            if !visited.contains(&node) {
                let location = graph
                    .node_positions
                    .get(&node)
                    .map(|p| (p.x, p.y))
                    .unwrap_or((0, 0));
                violations.push(ConnectivityViolation::DisconnectedPin {
                    net_id,
                    pin_id: node,
                    location,
                });
            }
        }
    }

    violations
}

/// Detect shorts between different nets by checking if nodes from different nets
/// are within snap distance.
fn detect_shorts(graph: &ConnectivityGraph) -> Vec<ConnectivityViolation> {
    let mut violations = Vec::new();
    let mut checked_pairs: HashSet<(u32, u32)> = HashSet::new();

    let nodes_a: Vec<(&u32, &u32)> = graph.node_nets.iter().collect();

    for (&node_a, &net_a) in &nodes_a {
        let pos_a = match graph.node_positions.get(&node_a) {
            Some(p) => *p,
            None => continue,
        };
        for (&node_b, &net_b) in &nodes_a {
            if node_a >= node_b || net_a == net_b {
                continue;
            }
            let pair_key = (node_a.min(node_b), node_a.max(node_b));
            if !checked_pairs.insert(pair_key) {
                continue;
            }
            if let Some(pos_b) = graph.node_positions.get(&node_b) {
                if points_snapped(&pos_a, pos_b) {
                    violations.push(ConnectivityViolation::UnwaivedShort {
                        net_a,
                        net_b,
                        location: (pos_a.x, pos_a.y),
                    });
                }
            }
        }
    }

    violations
}

/// Full connectivity verification: build graph, check reachability, detect shorts.
pub fn verify_connectivity(
    segments: &[IndexedSegment],
    junctions: &[VirtualJunction],
) -> ConnectivityResult {
    let start = std::time::Instant::now();

    let graph = build_connectivity_graph(segments, junctions);

    let mut violations = check_reachability(&graph);
    let short_violations = detect_shorts(&graph);
    violations.extend(short_violations);

    let nets_checked = graph.net_nodes.len() as u32;
    let pins_verified = graph.node_positions.len() as u32;

    let elapsed = start.elapsed();
    let time_ms = elapsed.as_millis() as u64;

    ConnectivityResult {
        violations,
        nets_checked,
        pins_verified,
        time_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point3D;
    use crate::geometry_router::route_decomposition::VirtualJunction;
    use crate::geometry_router::spatial_index::IndexedSegment;
    use crate::netlist::NetId;

    fn make_segment(
        segment_id: usize,
        net_id: usize,
        start: Point3D,
        end: Point3D,
    ) -> IndexedSegment {
        IndexedSegment {
            segment_id,
            net_id,
            width_nm: 200_000,
            thickness_nm: 35_000,
            start,
            end,
            layer: 1,
        }
    }

    #[test]
    fn test_fully_connected_net_no_violations() {
        let seg0 = make_segment(0, 1, Point3D::new(0, 0, 1), Point3D::new(5_000_000, 0, 1));
        let seg1 = make_segment(
            1,
            1,
            Point3D::new(5_000_000, 0, 1),
            Point3D::new(10_000_000, 0, 1),
        );
        let segments = vec![seg0, seg1];
        let junctions: Vec<VirtualJunction> = vec![];

        let result = verify_connectivity(&segments, &junctions);
        assert!(
            result.violations.is_empty(),
            "Expected no violations, got {:?}",
            result.violations
        );
        assert_eq!(result.nets_checked, 1);
        assert_eq!(result.pins_verified, 3);
    }

    #[test]
    fn test_disconnected_pin_violation() {
        let seg0 = make_segment(0, 1, Point3D::new(0, 0, 1), Point3D::new(5_000_000, 0, 1));
        let seg1 = make_segment(
            1,
            1,
            Point3D::new(20_000_000, 0, 1),
            Point3D::new(25_000_000, 0, 1),
        );
        let segments = vec![seg0, seg1];
        let junctions: Vec<VirtualJunction> = vec![];

        let result = verify_connectivity(&segments, &junctions);
        let disconnected = result
            .violations
            .iter()
            .filter(|v| matches!(v, ConnectivityViolation::DisconnectedPin { .. }))
            .count();
        assert!(
            disconnected >= 2,
            "Expected at least 2 DisconnectedPin violations, got {disconnected}"
        );
    }

    #[test]
    fn test_short_between_nets() {
        let shared_point = Point3D::new(5_000_000, 0, 1);
        let seg_a = make_segment(0, 1, Point3D::new(0, 0, 1), shared_point);
        let seg_b = make_segment(1, 2, Point3D::new(10_000_000, 0, 1), shared_point);
        let segments = vec![seg_a, seg_b];
        let junctions: Vec<VirtualJunction> = vec![];

        let result = verify_connectivity(&segments, &junctions);
        let shorts = result
            .violations
            .iter()
            .filter(|v| matches!(v, ConnectivityViolation::UnwaivedShort { .. }))
            .count();
        assert_eq!(shorts, 1, "Expected 1 UnwaivedShort violation");
    }

    #[test]
    fn test_broken_net() {
        let segments: Vec<IndexedSegment> = vec![];
        let junctions: Vec<VirtualJunction> = vec![];

        let graph = build_connectivity_graph(&segments, &junctions);
        assert!(graph.net_nodes.is_empty());
        assert!(graph.adjacency.is_empty());
    }

    #[test]
    fn test_connectivity_with_junction() {
        let seg0 = make_segment(0, 1, Point3D::new(0, 0, 1), Point3D::new(5_000_000, 0, 1));
        let seg1 = make_segment(
            1,
            1,
            Point3D::new(5_000_000, 0, 1),
            Point3D::new(10_000_000, 0, 1),
        );
        let seg2 = make_segment(
            2,
            1,
            Point3D::new(5_000_000, 0, 1),
            Point3D::new(5_000_000, 5_000_000, 1),
        );
        let segments = vec![seg0, seg1, seg2];

        let junc = VirtualJunction {
            junction_id: 0,
            position: Point3D::new(5_000_000, 0, 1),
            connected_segments: vec![0, 1, 2],
            net_id: NetId::new(1),
            capacitance_pf: 0.0,
            inductance_nh: 0.0,
        };
        let junctions = vec![junc];

        let result = verify_connectivity(&segments, &junctions);
        assert!(
            result.violations.is_empty(),
            "Expected no violations, got {:?}",
            result.violations
        );
        assert_eq!(result.pins_verified, 4);
    }
}
