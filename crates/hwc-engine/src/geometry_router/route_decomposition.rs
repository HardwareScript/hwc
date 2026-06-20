use crate::geometry::Point3D;
use crate::netlist::NetId;

/// A pin node with global coordinates, extracted from the netlist.
#[derive(Clone, Debug)]
pub struct PinNode {
    pub pin_id: usize,
    pub component_name: String,
    pub pin_name: String,
    pub position: Point3D,
    pub net_id: NetId,
}

/// A single point-to-point route segment (edge in the MST).
#[derive(Clone, Debug)]
pub struct RouteSegment {
    pub segment_id: usize,
    pub net_id: NetId,
    pub from_pin: PinNode,
    pub to_pin: PinNode,
    pub distance: f64,
}

/// A virtual junction node where two route segments meet (T-junction tap).
#[derive(Clone, Debug)]
pub struct VirtualJunction {
    pub junction_id: usize,
    pub position: Point3D,
    pub connected_segments: Vec<usize>,
    pub net_id: NetId,
    pub capacitance_pf: f64,
    pub inductance_nh: f64,
}

/// Result of decomposing a multi-pin net into route segments.
#[derive(Clone, Debug)]
pub struct DecomposedNet {
    pub net_id: NetId,
    pub pin_nodes: Vec<PinNode>,
    pub segments: Vec<RouteSegment>,
    pub junctions: Vec<VirtualJunction>,
}

/// Decompose a multi-pin net into point-to-point route segments using MST.
pub fn decompose_net(
    net_id: NetId,
    pin_nodes: Vec<PinNode>,
    next_segment_id: &mut usize,
    next_junction_id: &mut usize,
) -> DecomposedNet {
    let mut segments = Vec::new();
    let mut junctions = Vec::new();

    match pin_nodes.len() {
        0 | 1 => {}
        2 => {
            let from = pin_nodes[0].clone();
            let to = pin_nodes[1].clone();
            let distance = euclidean_distance(&from.position, &to.position);
            segments.push(RouteSegment {
                segment_id: *next_segment_id,
                net_id,
                from_pin: from,
                to_pin: to,
                distance,
            });
            *next_segment_id += 1;
        }
        _ => {
            let matrix = distance_matrix(&pin_nodes);
            let edges = prim_mst(&matrix);

            for (from_idx, to_idx) in edges {
                let from = pin_nodes[from_idx].clone();
                let to = pin_nodes[to_idx].clone();
                let distance = euclidean_distance(&from.position, &to.position);
                segments.push(RouteSegment {
                    segment_id: *next_segment_id,
                    net_id,
                    from_pin: from,
                    to_pin: to,
                    distance,
                });
                *next_segment_id += 1;
            }
        }
    }

    if !segments.is_empty() {
        junctions = detect_junctions(&segments, next_junction_id);
    }

    DecomposedNet {
        net_id,
        pin_nodes,
        segments,
        junctions,
    }
}

/// Collect pin nodes from the netlist for a specific net.
pub fn collect_pin_nodes(
    net_id: NetId,
    arena: &crate::netlist::NetlistArena,
) -> Vec<PinNode> {
    let pins = match arena.get_net_pins(net_id) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut result = Vec::with_capacity(pins.len());

    for &pin_id in pins {
        let pin_data = match arena.get_pin(pin_id) {
            Some(p) => p,
            None => continue,
        };

        let comp_data = match arena.get_component(pin_data.parent_component) {
            Some(c) => c,
            None => continue,
        };

        let position = match arena.get_pin_position(pin_id) {
            Some(pos) => Point3D::new(pos.0, pos.1, pos.2),
            None => continue,
        };

        result.push(PinNode {
            pin_id: pin_id.raw() as usize,
            component_name: comp_data.name.to_string(),
            pin_name: pin_data.name.to_string(),
            position,
            net_id,
        });
    }

    result
}

/// Compute Euclidean distance between two points.
#[inline]
pub fn euclidean_distance(a: &Point3D, b: &Point3D) -> f64 {
    let dx = (a.x - b.x) as f64;
    let dy = (a.y - b.y) as f64;
    (dx * dx + dy * dy).sqrt()
}

/// Compute the complete distance matrix for a set of pin nodes.
pub fn distance_matrix(pins: &[PinNode]) -> Vec<Vec<f64>> {
    let n = pins.len();
    let mut matrix = vec![vec![0.0_f64; n]; n];

    for i in 0..n {
        for j in (i + 1)..n {
            let d = euclidean_distance(&pins[i].position, &pins[j].position);
            matrix[i][j] = d;
            matrix[j][i] = d;
        }
    }

    matrix
}

/// Prim's Minimum Spanning Tree algorithm.
/// Returns the list of (from_index, to_index) edges in the MST.
pub fn prim_mst(distance_matrix: &[Vec<f64>]) -> Vec<(usize, usize)> {
    let n = distance_matrix.len();
    if n == 0 {
        return Vec::new();
    }

    let mut visited = vec![false; n];
    let mut key = vec![f64::INFINITY; n];
    let mut parent = vec![usize::MAX; n];

    key[0] = 0.0;
    let mut edges = Vec::with_capacity(n.saturating_sub(1));

    for _ in 0..n {
        let mut u = usize::MAX;
        let mut min_key = f64::INFINITY;

        for v in 0..n {
            if !visited[v] && key[v] < min_key {
                min_key = key[v];
                u = v;
            }
        }

        if u == usize::MAX {
            break;
        }

        visited[u] = true;

        if parent[u] != usize::MAX {
            edges.push((parent[u], u));
        }

        for v in 0..n {
            if !visited[v] && distance_matrix[u][v] < key[v] {
                key[v] = distance_matrix[u][v];
                parent[v] = u;
            }
        }
    }

    edges
}

/// Detect and create VirtualJunction nodes for T-junction taps.
/// A T-junction occurs when a segment endpoint lies on another segment's interior.
pub fn detect_junctions(
    segments: &[RouteSegment],
    next_junction_id: &mut usize,
) -> Vec<VirtualJunction> {
    let tolerance = 1.0;
    let mut junctions = Vec::new();

    for (i, seg_a) in segments.iter().enumerate() {
        let endpoints = [
            (seg_a.from_pin.position, true),
            (seg_a.to_pin.position, false),
        ];

        for (endpoint_pos, _is_from) in &endpoints {
            for (j, seg_b) in segments.iter().enumerate() {
                if i == j {
                    continue;
                }

                if point_on_segment(*endpoint_pos, &seg_b.from_pin.position, &seg_b.to_pin.position, tolerance)
                    && *endpoint_pos != seg_b.from_pin.position
                    && *endpoint_pos != seg_b.to_pin.position
                {
                    let already_exists = junctions.iter().any(|j: &VirtualJunction| {
                        j.position == *endpoint_pos && j.net_id == seg_a.net_id
                    });

                    if !already_exists {
                        let connected = vec![seg_a.segment_id, seg_b.segment_id];
                        junctions.push(VirtualJunction {
                            junction_id: *next_junction_id,
                            position: *endpoint_pos,
                            connected_segments: connected,
                            net_id: seg_a.net_id,
                            capacitance_pf: 0.0,
                            inductance_nh: 0.0,
                        });
                        *next_junction_id += 1;
                    }
                }
            }
        }
    }

    junctions
}

fn point_on_segment(p: Point3D, a: &Point3D, b: &Point3D, tolerance: f64) -> bool {
    let cross = (p.y - a.y) as f64 * (b.x - a.x) as f64
        - (p.x - a.x) as f64 * (b.y - a.y) as f64;

    if cross.abs() > tolerance {
        return false;
    }

    let dot = (p.x - a.x) as f64 * (b.x - a.x) as f64
        + (p.y - a.y) as f64 * (b.y - a.y) as f64;

    if dot < 0.0 {
        return false;
    }

    let len_sq = (b.x - a.x) as f64 * (b.x - a.x) as f64
        + (b.y - a.y) as f64 * (b.y - a.y) as f64;

    if dot > len_sq {
        return false;
    }

    true
}
