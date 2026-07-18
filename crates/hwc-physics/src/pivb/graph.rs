use super::types::PlanarIsland;
use rustc_hash::FxHashMap;

/// Undirected connectivity graph for the PIVB solver.
///
/// Nodes are Planar Islands, edges are Vertical Bridges (vias/contacts).
/// The graph uses an adjacency list representation for efficient traversal.
pub struct ConnectivityGraph {
    /// Adjacency list: node_index -> list of neighbor node indices
    adj: Vec<Vec<usize>>,
    /// Maps node index to PlanarIsland
    nodes: Vec<PlanarIsland>,
    /// Maps PlanarIsland id -> graph node index
    id_to_index: FxHashMap<usize, usize>,
}

impl ConnectivityGraph {
    pub fn new() -> Self {
        Self {
            adj: Vec::new(),
            nodes: Vec::new(),
            id_to_index: FxHashMap::default(),
        }
    }

    /// Add a Planar Island node to the graph. Returns the graph node index.
    pub fn add_node(&mut self, island: PlanarIsland) -> usize {
        let idx = self.nodes.len();
        self.id_to_index.insert(island.id, idx);
        self.adj.push(Vec::new());
        self.nodes.push(island);
        idx
    }

    /// Add an undirected edge between two graph node indices.
    pub fn add_edge(&mut self, u: usize, v: usize) {
        if u < self.adj.len() && v < self.adj.len() && u != v {
            if !self.adj[u].contains(&v) {
                self.adj[u].push(v);
                self.adj[v].push(u);
            }
        }
    }

    /// Get the Planar Island at a given graph node index.
    pub fn node(&self, idx: usize) -> &PlanarIsland {
        &self.nodes[idx]
    }

    /// Get the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.adj.iter().map(|a| a.len()).sum::<usize>() / 2
    }

    /// Get a reference to all nodes.
    pub fn nodes(&self) -> &[PlanarIsland] {
        &self.nodes
    }

    /// Get adjacency list for a node.
    pub fn neighbors(&self, idx: usize) -> &[usize] {
        &self.adj[idx]
    }

    /// Find connected components using BFS.
    ///
    /// Returns a list of components, where each component is a list of node indices.
    /// A single connected component means the net is physically continuous.
    pub fn connected_components(&self) -> Vec<Vec<usize>> {
        let mut visited = vec![false; self.nodes.len()];
        let mut components = Vec::new();

        for start in 0..self.nodes.len() {
            if visited[start] {
                continue;
            }

            let mut component = Vec::new();
            let mut stack = vec![start];

            while let Some(curr) = stack.pop() {
                if visited[curr] {
                    continue;
                }
                visited[curr] = true;
                component.push(curr);

                for &neighbor in &self.adj[curr] {
                    if !visited[neighbor] {
                        stack.push(neighbor);
                    }
                }
            }

            components.push(component);
        }

        components
    }

    /// Find strongly connected components using Tarjan's algorithm.
    ///
    /// For an undirected graph, SCCs are equivalent to connected components.
    /// This is included for future directed-graph extensions (e.g., diode-aware
    /// connectivity).
    pub fn tarjan_scc(&self) -> Vec<Vec<usize>> {
        let n = self.nodes.len();
        if n == 0 {
            return Vec::new();
        }

        let mut index_counter = 0;
        let mut stack = Vec::new();
        let mut on_stack = vec![false; n];
        let mut indices = vec![-1i32; n];
        let mut lowlinks = vec![-1i32; n];
        let mut sccs = Vec::new();

        fn strongconnect(
            v: usize,
            adj: &[Vec<usize>],
            index_counter: &mut i32,
            stack: &mut Vec<usize>,
            on_stack: &mut [bool],
            indices: &mut [i32],
            lowlinks: &mut [i32],
            sccs: &mut Vec<Vec<usize>>,
        ) {
            indices[v] = *index_counter;
            lowlinks[v] = *index_counter;
            *index_counter += 1;
            stack.push(v);
            on_stack[v] = true;

            for &w in &adj[v] {
                if indices[w] == -1 {
                    strongconnect(
                        w,
                        adj,
                        index_counter,
                        stack,
                        on_stack,
                        indices,
                        lowlinks,
                        sccs,
                    );
                    lowlinks[v] = lowlinks[v].min(lowlinks[w]);
                } else if on_stack[w] {
                    lowlinks[v] = lowlinks[v].min(indices[w]);
                }
            }

            if lowlinks[v] == indices[v] {
                let mut scc = Vec::new();
                loop {
                    let w = stack.pop().expect("stack should not be empty");
                    on_stack[w] = false;
                    scc.push(w);
                    if w == v {
                        break;
                    }
                }
                sccs.push(scc);
            }
        }

        for v in 0..n {
            if indices[v] == -1 {
                strongconnect(
                    v,
                    &self.adj,
                    &mut index_counter,
                    &mut stack,
                    &mut on_stack,
                    &mut indices,
                    &mut lowlinks,
                    &mut sccs,
                );
            }
        }

        sccs
    }
}

impl Default for ConnectivityGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{BoundingBox, Point3D};

    fn make_island(id: usize, net: &str) -> PlanarIsland {
        PlanarIsland {
            id,
            layer_name: "top_copper".into(),
            z_min: 0,
            z_max: 100,
            boundary: BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(1000, 1000, 100)),
            bbox: BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(1000, 1000, 100)),
            center: Point3D::new(500, 500, 50),
            net_name: net.into(),
            net_id: 1,
            material: 2,
        }
    }

    #[test]
    fn test_single_component() {
        let mut graph = ConnectivityGraph::new();
        graph.add_node(make_island(0, "VCC"));
        graph.add_node(make_island(1, "VCC"));
        graph.add_node(make_island(2, "VCC"));
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);

        let components = graph.connected_components();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].len(), 3);
    }

    #[test]
    fn test_two_components() {
        let mut graph = ConnectivityGraph::new();
        graph.add_node(make_island(0, "VCC"));
        graph.add_node(make_island(1, "VCC"));
        graph.add_node(make_island(2, "VCC"));
        graph.add_edge(0, 1);
        // Node 2 is disconnected

        let components = graph.connected_components();
        assert_eq!(components.len(), 2);
    }

    #[test]
    fn test_tarjan_scc_matches_connected_components() {
        let mut graph = ConnectivityGraph::new();
        graph.add_node(make_island(0, "VCC"));
        graph.add_node(make_island(1, "VCC"));
        graph.add_node(make_island(2, "VCC"));
        graph.add_node(make_island(3, "VCC"));
        graph.add_edge(0, 1);
        graph.add_edge(2, 3);

        let cc = graph.connected_components();
        let scc = graph.tarjan_scc();

        // For undirected graphs, SCCs == connected components
        assert_eq!(cc.len(), scc.len());
    }

    #[test]
    fn test_empty_graph() {
        let graph = ConnectivityGraph::new();
        let components = graph.connected_components();
        assert_eq!(components.len(), 0);

        let scc = graph.tarjan_scc();
        assert_eq!(scc.len(), 0);
    }

    #[test]
    fn test_single_node() {
        let mut graph = ConnectivityGraph::new();
        graph.add_node(make_island(0, "VCC"));

        let components = graph.connected_components();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0], vec![0]);
    }

    #[test]
    fn test_self_loop_ignored() {
        let mut graph = ConnectivityGraph::new();
        graph.add_node(make_island(0, "VCC"));
        graph.add_edge(0, 0);

        let components = graph.connected_components();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0], vec![0]);
    }
}
