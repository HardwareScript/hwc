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
        if u < self.adj.len() && v < self.adj.len() && u != v && !self.adj[u].contains(&v) {
            self.adj[u].push(v);
            self.adj[v].push(u);
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

        let mut state = TarjanState {
            adj: &self.adj,
            index_counter: 0,
            stack: Vec::new(),
            on_stack: vec![false; n],
            indices: vec![-1i32; n],
            lowlinks: vec![-1i32; n],
            sccs: Vec::new(),
        };

        for v in 0..n {
            if state.indices[v] == -1 {
                state.strongconnect(v);
            }
        }

        state.sccs
    }
}

struct TarjanState<'a> {
    adj: &'a [Vec<usize>],
    index_counter: i32,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    indices: Vec<i32>,
    lowlinks: Vec<i32>,
    sccs: Vec<Vec<usize>>,
}

impl TarjanState<'_> {
    fn strongconnect(&mut self, v: usize) {
        self.indices[v] = self.index_counter;
        self.lowlinks[v] = self.index_counter;
        self.index_counter += 1;
        self.stack.push(v);
        self.on_stack[v] = true;

        for &w in &self.adj[v] {
            if self.indices[w] == -1 {
                self.strongconnect(w);
                self.lowlinks[v] = self.lowlinks[v].min(self.lowlinks[w]);
            } else if self.on_stack[w] {
                self.lowlinks[v] = self.lowlinks[v].min(self.indices[w]);
            }
        }

        if self.lowlinks[v] == self.indices[v] {
            let mut scc = Vec::new();
            loop {
                let w = self.stack.pop().expect("stack should not be empty");
                self.on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            self.sccs.push(scc);
        }
    }
}

impl Default for ConnectivityGraph {
    fn default() -> Self {
        Self::new()
    }
}
