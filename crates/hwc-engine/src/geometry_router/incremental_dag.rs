use rustc_hash::FxHashMap;
use std::collections::VecDeque;

/// The type of change that triggered an invalidation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DependencyType {
    ComponentMove,
    PinRelocation,
    SpatialCorridorChange,
    NetModification,
}

/// Identifies what a dependency node represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeType {
    Component { component_id: u32 },
    Net { net_id: u32 },
    GCell { gx: i32, gy: i32 },
    RouteSegment { net_id: u32, segment_idx: usize },
}

/// A single node in the dependency DAG.
#[derive(Clone, Debug)]
pub struct DependencyNode {
    pub id: u64,
    pub node_type: NodeType,
    pub dependencies: Vec<u64>,
    pub dependents: Vec<u64>,
    pub dirty: bool,
}

/// The incremental dependency DAG for tracking which routing artifacts
/// need re-routing after a change.
#[derive(Debug)]
pub struct DependencyDag {
    pub nodes: FxHashMap<u64, DependencyNode>,
}

/// Plan produced by `plan_reroute` describing what needs re-routing.
#[derive(Clone, Debug)]
pub struct ReroutePlan {
    pub dirty_nets: Vec<u32>,
    pub dirty_gcells: Vec<(i32, i32)>,
    pub locked_nets: Vec<u32>,
    pub estimated_segments: usize,
}

/// Statistics about the DAG.
#[derive(Clone, Debug)]
pub struct DagStats {
    pub total_nodes: usize,
    pub dirty_nodes: usize,
    pub total_edges: usize,
    pub max_depth: usize,
}

/// Error returned when a cycle is detected in the DAG.
#[derive(Clone, Debug)]
pub struct CycleError {
    pub message: String,
    pub cycle_nodes: Vec<u64>,
}

/// Error returned when a dependency references a non-existent node.
#[derive(Clone, Debug)]
pub struct MissingDependencyError {
    pub message: String,
    pub missing_ids: Vec<u64>,
}

impl DependencyDag {
    /// Create an empty DAG.
    pub fn new() -> Self {
        Self {
            nodes: FxHashMap::default(),
        }
    }

    /// Compute a deterministic node ID from a `NodeType`.
    ///
    /// Uses a simple multiplicative hash that is stable across runs.
    #[inline]
    pub fn compute_node_id(node_type: &NodeType) -> u64 {
        match *node_type {
            NodeType::Component { component_id } => {
                // 0x01 << 48 | component_id as lower bits
                0x01_0000_0000_0000 | (component_id as u64)
            }
            NodeType::Net { net_id } => {
                0x02_0000_0000_0000 | (net_id as u64)
            }
            NodeType::GCell { gx, gy } => {
                let gx_u = gx as u32;
                let gy_u = gy as u32;
                0x03_0000_0000_0000 | ((gx_u as u64) << 32) | (gy_u as u64)
            }
            NodeType::RouteSegment { net_id, segment_idx } => {
                0x04_0000_0000_0000 | ((net_id as u64) << 16) | (segment_idx as u64)
            }
        }
    }

    /// Insert a node into the DAG, creating it if it does not exist.
    /// Returns the node ID.
    fn ensure_node(&mut self, node_type: NodeType) -> u64 {
        let id = Self::compute_node_id(&node_type);
        if !self.nodes.contains_key(&id) {
            let node = DependencyNode {
                id,
                node_type,
                dependencies: Vec::new(),
                dependents: Vec::new(),
                dirty: false,
            };
            self.nodes.insert(id, node);
        }
        id
    }

    /// Register a component and its net dependencies.
    ///
    /// Creates edges: component -> each net (component depends on net).
    pub fn register_component(&mut self, component_id: u32, connected_nets: &[u32]) {
        let comp_id = self.ensure_node(NodeType::Component { component_id });

        // Create the net nodes if missing
        for &nid in connected_nets {
            self.ensure_node(NodeType::Net { net_id: nid });
        }

        // Compute node IDs for wiring
        let net_ids: Vec<u64> = connected_nets
            .iter()
            .map(|&nid| Self::compute_node_id(&NodeType::Net { net_id: nid }))
            .collect();

        // Wire: component depends on nets
        if let Some(comp) = self.nodes.get_mut(&comp_id) {
            comp.dependencies = net_ids.clone();
        }

        // Wire: nets list component as dependent
        for &nid in &net_ids {
            if let Some(net_node) = self.nodes.get_mut(&nid) {
                if !net_node.dependents.contains(&comp_id) {
                    net_node.dependents.push(comp_id);
                }
            }
        }
    }

    /// Register a net and its G-cell dependencies.
    ///
    /// Creates edges: net -> each GCell (net depends on GCell).
    pub fn register_net(&mut self, net_id: u32, gcells: &[(i32, i32)]) {
        let net_nid = self.ensure_node(NodeType::Net { net_id });

        // Create G-cell nodes if missing
        for &(gx, gy) in gcells {
            self.ensure_node(NodeType::GCell { gx, gy });
        }

        // Compute node IDs for wiring
        let gcell_ids: Vec<u64> = gcells
            .iter()
            .map(|&(gx, gy)| Self::compute_node_id(&NodeType::GCell { gx, gy }))
            .collect();

        // Wire: net depends on G-cells
        if let Some(net) = self.nodes.get_mut(&net_nid) {
            net.dependencies = gcell_ids.clone();
        }

        // Wire: G-cells list net as dependent
        for &gid in &gcell_ids {
            if let Some(gcell) = self.nodes.get_mut(&gid) {
                if !gcell.dependents.contains(&net_nid) {
                    gcell.dependents.push(net_nid);
                }
            }
        }
    }

    /// Register a route segment and its dependencies.
    ///
    /// Creates edges: route_segment -> components and GCells it depends on.
    pub fn register_route_segment(
        &mut self,
        net_id: u32,
        segment_idx: usize,
        depends_on_components: &[u32],
        depends_on_gcells: &[(i32, i32)],
    ) {
        let seg_id = self.ensure_node(NodeType::RouteSegment { net_id, segment_idx });
        let mut dep_ids: Vec<u64> = Vec::new();

        for &cid in depends_on_components {
            let comp_id = Self::compute_node_id(&NodeType::Component { component_id: cid });
            self.ensure_node(NodeType::Component { component_id: cid });
            dep_ids.push(comp_id);
        }

        for &(gx, gy) in depends_on_gcells {
            let gid = Self::compute_node_id(&NodeType::GCell { gx, gy });
            self.ensure_node(NodeType::GCell { gx, gy });
            dep_ids.push(gid);
        }

        // Wire: segment depends on these nodes
        if let Some(seg) = self.nodes.get_mut(&seg_id) {
            seg.dependencies = dep_ids.clone();
        }

        // Wire: each dependency lists segment as dependent
        for &did in &dep_ids {
            if let Some(dep) = self.nodes.get_mut(&did) {
                if !dep.dependents.contains(&seg_id) {
                    dep.dependents.push(seg_id);
                }
            }
        }
    }

    // ── Granular Invalidation ──────────────────────────────────────

    /// Mark a component dirty and propagate to all reachable nodes (dependents).
    ///
    /// Returns the list of all node IDs that became dirty.
    #[inline]
    pub fn invalidate_component(&mut self, component_id: u32) -> Vec<u64> {
        let root_id = Self::compute_node_id(&NodeType::Component { component_id });
        self.mark_dirty_transitive(root_id)
    }

    /// Mark a G-cell dirty and propagate to all dependent nets.
    #[inline]
    pub fn invalidate_gcell(&mut self, gx: i32, gy: i32) -> Vec<u64> {
        let root_id = Self::compute_node_id(&NodeType::GCell { gx, gy });
        self.mark_dirty_transitive(root_id)
    }

    /// Mark a net dirty and propagate to all dependent route segments.
    #[inline]
    pub fn invalidate_net(&mut self, net_id: u32) -> Vec<u64> {
        let root_id = Self::compute_node_id(&NodeType::Net { net_id });
        self.mark_dirty_transitive(root_id)
    }

    /// BFS traversal: mark the root dirty and all nodes reachable through
    /// `dependents` and `dependencies` edges. Returns all newly-dirtied node IDs.
    #[inline]
    fn mark_dirty_transitive(&mut self, root_id: u64) -> Vec<u64> {
        let mut dirty = Vec::new();
        let mut queue = VecDeque::new();

        // Mark root
        if let Some(node) = self.nodes.get_mut(&root_id) {
            if !node.dirty {
                node.dirty = true;
                dirty.push(root_id);
                // Enqueue both dependents and dependencies
                let dependents = node.dependents.clone();
                for &dep in &dependents {
                    queue.push_back(dep);
                }
                let dependencies = node.dependencies.clone();
                for &dep in &dependencies {
                    queue.push_back(dep);
                }
            }
        }

        // BFS through both dependents and dependencies
        while let Some(nid) = queue.pop_front() {
            if let Some(node) = self.nodes.get_mut(&nid) {
                if !node.dirty {
                    node.dirty = true;
                    dirty.push(nid);
                    let dependents = node.dependents.clone();
                    for &dep in &dependents {
                        queue.push_back(dep);
                    }
                    let dependencies = node.dependencies.clone();
                    for &dep in &dependencies {
                        queue.push_back(dep);
                    }
                }
            }
        }

        dirty
    }

    // ── Dirty Node Collection ──────────────────────────────────────

    /// Return all nodes marked dirty.
    #[inline]
    pub fn get_dirty_nodes(&self) -> Vec<&DependencyNode> {
        self.nodes.values().filter(|n| n.dirty).collect()
    }

    /// Return unique net IDs that are dirty.
    pub fn get_dirty_nets(&self) -> Vec<u32> {
        let mut seen = rustc_hash::FxHashSet::default();
        let mut result = Vec::new();
        for node in self.nodes.values() {
            if node.dirty {
                if let NodeType::Net { net_id } = node.node_type {
                    if seen.insert(net_id) {
                        result.push(net_id);
                    }
                }
            }
        }
        result
    }

    /// Return unique G-cells that are dirty.
    pub fn get_dirty_gcells(&self) -> Vec<(i32, i32)> {
        let mut seen = rustc_hash::FxHashSet::default();
        let mut result = Vec::new();
        for node in self.nodes.values() {
            if node.dirty {
                if let NodeType::GCell { gx, gy } = node.node_type {
                    if seen.insert((gx, gy)) {
                        result.push((gx, gy));
                    }
                }
            }
        }
        result
    }

    // ── Incremental Re-route ───────────────────────────────────────

    /// Produce a re-route plan from the current dirty state.
    ///
    /// Only dirty, unlocked nets are included. Clean nets are locked.
    pub fn plan_reroute(&self) -> ReroutePlan {
        let dirty_nets = self.get_dirty_nets();
        let dirty_gcells = self.get_dirty_gcells();

        // Locked nets = all net nodes that are NOT dirty
        let mut locked_nets = Vec::new();
        let mut seen_locked = rustc_hash::FxHashSet::default();
        for node in self.nodes.values() {
            if !node.dirty {
                if let NodeType::Net { net_id } = node.node_type {
                    if seen_locked.insert(net_id) {
                        locked_nets.push(net_id);
                    }
                }
            }
        }

        // Count segments on dirty nets
        let estimated_segments: usize = self
            .nodes
            .values()
            .filter(|n| n.dirty && matches!(n.node_type, NodeType::Net { .. }))
            .map(|n| {
                self.nodes
                    .values()
                    .filter(|r| {
                        r.dirty
                            && matches!(r.node_type, NodeType::RouteSegment { .. })
                            && r.dependencies.contains(&n.id)
                    })
                    .count()
            })
            .sum();

        ReroutePlan {
            dirty_nets,
            dirty_gcells,
            locked_nets,
            estimated_segments,
        }
    }

    /// Clear dirty flags for the specified node IDs.
    #[inline]
    pub fn mark_clean(&mut self, node_ids: &[u64]) {
        for &nid in node_ids {
            if let Some(node) = self.nodes.get_mut(&nid) {
                node.dirty = false;
            }
        }
    }

    // ── DAG Integrity Verification ─────────────────────────────────

    /// DFS-based cycle detection.
    ///
    /// Returns `Ok(())` if the graph is acyclic, or `Err(CycleError)` with
    /// the list of nodes involved in the cycle.
    pub fn verify_no_cycles(&self) -> Result<(), CycleError> {
        // 0 = unvisited, 1 = in-progress (on DFS stack), 2 = done
        let mut state: FxHashMap<u64, u8> = FxHashMap::default();
        for &id in self.nodes.keys() {
            state.insert(id, 0);
        }

        let mut stack: Vec<u64> = Vec::new();

        for &start_id in self.nodes.keys() {
            if *state.get(&start_id).unwrap_or(&2) != 0 {
                continue;
            }

            stack.push(start_id);

            while let Some(&current) = stack.last() {
                let s = state.get(&current).copied().unwrap_or(2);
                if s == 0 {
                    state.insert(current, 1); // mark in-progress
                    // Push unvisited dependents
                    if let Some(node) = self.nodes.get(&current) {
                        let deps = node.dependents.clone();
                        let mut pushed = false;
                        for &dep in &deps {
                            let ds = state.get(&dep).copied().unwrap_or(2);
                            if ds == 0 {
                                stack.push(dep);
                                pushed = true;
                            } else if ds == 1 {
                                // Found a cycle — collect cycle path
                                let mut cycle = vec![dep, current];
                                // Walk stack to find cycle start
                                for &s_node in stack.iter().rev().skip(1) {
                                    cycle.push(s_node);
                                    if s_node == dep {
                                        break;
                                    }
                                }
                                cycle.reverse();
                                return Err(CycleError {
                                    message: format!(
                                        "Cycle detected involving {} nodes",
                                        cycle.len()
                                    ),
                                    cycle_nodes: cycle,
                                });
                            }
                        }
                        if !pushed {
                            state.insert(current, 2); // done
                            stack.pop();
                        }
                    } else {
                        state.insert(current, 2);
                        stack.pop();
                    }
                } else {
                    // Already processing or done — pop
                    stack.pop();
                }
            }
        }

        Ok(())
    }

    /// Ensure all dependency and dependent references point to existing nodes.
    pub fn verify_all_dependents_present(&self) -> Result<(), MissingDependencyError> {
        let mut missing = Vec::new();

        for node in self.nodes.values() {
            for &dep_id in &node.dependencies {
                if !self.nodes.contains_key(&dep_id) {
                    missing.push(dep_id);
                }
            }
            for &dep_id in &node.dependents {
                if !self.nodes.contains_key(&dep_id) {
                    missing.push(dep_id);
                }
            }
        }

        missing.sort_unstable();
        missing.dedup();

        if missing.is_empty() {
            Ok(())
        } else {
            Err(MissingDependencyError {
                message: format!(
                    "{} referenced node(s) not found in DAG",
                    missing.len()
                ),
                missing_ids: missing,
            })
        }
    }

    // ── Statistics ─────────────────────────────────────────────────

    /// Return statistics about the DAG.
    pub fn stats(&self) -> DagStats {
        let total_nodes = self.nodes.len();
        let dirty_nodes = self.nodes.values().filter(|n| n.dirty).count();
        let total_edges: usize = self
            .nodes
            .values()
            .map(|n| n.dependencies.len())
            .sum();

        // Compute max depth via BFS from root nodes (nodes with no dependents)
        let max_depth = self.compute_max_depth();

        DagStats {
            total_nodes,
            dirty_nodes,
            total_edges,
            max_depth,
        }
    }

    /// Compute the maximum depth of the DAG (longest path from a leaf to a root).
    fn compute_max_depth(&self) -> usize {
        if self.nodes.is_empty() {
            return 0;
        }

        // Topological order via Kahn's algorithm on reversed edges
        // Root = node with no dependents (top of dependency chain)
        let mut in_degree: FxHashMap<u64, usize> = FxHashMap::default();
        for (&id, _) in &self.nodes {
            in_degree.entry(id).or_insert(0);
        }

        // in_degree counts how many *dependents* point to a node
        // (i.e., how many nodes this node is "above" in the chain)
        for node in self.nodes.values() {
            for &dep in &node.dependents {
                *in_degree.entry(dep).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<u64> = VecDeque::new();
        let mut depth: FxHashMap<u64, usize> = FxHashMap::default();

        // Nodes with in_degree 0 in reversed graph = leaf nodes
        for (&id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(id);
                depth.insert(id, 1);
            }
        }

        let mut max_d = 0usize;
        while let Some(nid) = queue.pop_front() {
            let d = depth.get(&nid).copied().unwrap_or(1);
            if d > max_d {
                max_d = d;
            }
            if let Some(node) = self.nodes.get(&nid) {
                // node.dependents are nodes that depend on nid
                // In reversed graph, we go from nid -> each node in dependencies
                // (because "dependencies" are below nid)
                for &dep in &node.dependencies {
                    let nd = d + 1;
                    if nd > *depth.get(&dep).unwrap_or(&0) {
                        depth.insert(dep, nd);
                        let deg = in_degree.entry(dep).or_insert(0);
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        max_d
    }
}

impl Default for DependencyDag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(unwrap_used, expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_deterministic() {
        let a = DependencyDag::compute_node_id(&NodeType::Component { component_id: 42 });
        let b = DependencyDag::compute_node_id(&NodeType::Component { component_id: 42 });
        assert_eq!(a, b);

        let c = DependencyDag::compute_node_id(&NodeType::Net { net_id: 7 });
        let d = DependencyDag::compute_node_id(&NodeType::Net { net_id: 7 });
        assert_eq!(c, d);

        let e = DependencyDag::compute_node_id(&NodeType::GCell { gx: 3, gy: -1 });
        let f = DependencyDag::compute_node_id(&NodeType::GCell { gx: 3, gy: -1 });
        assert_eq!(e, f);

        let g = DependencyDag::compute_node_id(&NodeType::RouteSegment { net_id: 5, segment_idx: 2 });
        let h = DependencyDag::compute_node_id(&NodeType::RouteSegment { net_id: 5, segment_idx: 2 });
        assert_eq!(g, h);

        // Different types produce different IDs
        assert_ne!(a, c);
        assert_ne!(a, e);
        assert_ne!(c, e);
    }

    #[test]
    fn test_register_component_creates_dependencies() {
        let mut dag = DependencyDag::new();
        dag.register_component(1, &[10, 20]);

        let comp_id = DependencyDag::compute_node_id(&NodeType::Component { component_id: 1 });
        let net10_id = DependencyDag::compute_node_id(&NodeType::Net { net_id: 10 });
        let net20_id = DependencyDag::compute_node_id(&NodeType::Net { net_id: 20 });

        let comp = dag.nodes.get(&comp_id).expect("component node exists");
        assert_eq!(comp.dependencies.len(), 2);
        assert!(comp.dependencies.contains(&net10_id));
        assert!(comp.dependencies.contains(&net20_id));

        // Net nodes should list component as dependent
        let net10 = dag.nodes.get(&net10_id).expect("net 10 node exists");
        assert!(net10.dependents.contains(&comp_id));

        let net20 = dag.nodes.get(&net20_id).expect("net 20 node exists");
        assert!(net20.dependents.contains(&comp_id));
    }

    #[test]
    fn test_invalidate_component_marks_connected_nets_dirty() {
        let mut dag = DependencyDag::new();
        dag.register_component(1, &[10, 20]);

        let dirty = dag.invalidate_component(1);
        assert!(!dirty.is_empty());

        // Component should be dirty
        let comp_id = DependencyDag::compute_node_id(&NodeType::Component { component_id: 1 });
        assert!(dag.nodes.get(&comp_id).unwrap().dirty);

        // Both nets should be dirty
        let net10_id = DependencyDag::compute_node_id(&NodeType::Net { net_id: 10 });
        assert!(dag.nodes.get(&net10_id).unwrap().dirty);

        let net20_id = DependencyDag::compute_node_id(&NodeType::Net { net_id: 20 });
        assert!(dag.nodes.get(&net20_id).unwrap().dirty);
    }

    #[test]
    fn test_invalidate_gcell_marks_dependent_nets_dirty() {
        let mut dag = DependencyDag::new();
        dag.register_net(100, &[(0, 0), (1, 0)]);

        let dirty = dag.invalidate_gcell(0, 0);
        assert!(!dirty.is_empty());

        let gcell_id = DependencyDag::compute_node_id(&NodeType::GCell { gx: 0, gy: 0 });
        assert!(dag.nodes.get(&gcell_id).unwrap().dirty);

        let net_id = DependencyDag::compute_node_id(&NodeType::Net { net_id: 100 });
        assert!(dag.nodes.get(&net_id).unwrap().dirty);
    }

    #[test]
    fn test_get_dirty_nodes_returns_only_dirty() {
        let mut dag = DependencyDag::new();
        dag.register_component(1, &[10]);
        dag.register_component(2, &[20]);

        dag.invalidate_component(1);

        let dirty = dag.get_dirty_nodes();
        let dirty_ids: Vec<u64> = dirty.iter().map(|n| n.id).collect();

        let comp1_id = DependencyDag::compute_node_id(&NodeType::Component { component_id: 1 });
        let comp2_id = DependencyDag::compute_node_id(&NodeType::Component { component_id: 2 });

        assert!(dirty_ids.contains(&comp1_id));
        assert!(!dirty_ids.contains(&comp2_id));
    }

    #[test]
    fn test_plan_reroute_excludes_clean_nets() {
        let mut dag = DependencyDag::new();
        dag.register_net(1, &[(0, 0)]);
        dag.register_net(2, &[(1, 1)]);

        dag.invalidate_net(1);

        let plan = dag.plan_reroute();
        assert!(plan.dirty_nets.contains(&1));
        assert!(!plan.dirty_nets.contains(&2));
        assert!(plan.locked_nets.contains(&2));
        assert!(!plan.locked_nets.contains(&1));
    }

    #[test]
    fn test_mark_clean_clears_dirty_flags() {
        let mut dag = DependencyDag::new();
        dag.register_component(1, &[10]);

        let dirty = dag.invalidate_component(1);
        assert!(!dirty.is_empty());

        dag.mark_clean(&dirty);

        for &nid in &dirty {
            let node = dag.nodes.get(&nid).expect("node exists");
            assert!(!node.dirty, "node {} should be clean", nid);
        }
    }

    #[test]
    fn test_verify_no_cycles_passes_on_acyclic() {
        let mut dag = DependencyDag::new();
        dag.register_component(1, &[10, 20]);
        dag.register_net(10, &[(0, 0)]);
        dag.register_net(20, &[(1, 1)]);

        assert!(dag.verify_no_cycles().is_ok());
    }

    #[test]
    fn test_verify_no_cycles_detects_cycle() {
        let mut dag = DependencyDag::new();
        // Manually construct a cycle: A -> B -> A
        let id_a = 100u64;
        let id_b = 200u64;

        let node_a = DependencyNode {
            id: id_a,
            node_type: NodeType::Component { component_id: 99 },
            dependencies: vec![id_b],
            dependents: vec![id_b],
            dirty: false,
        };
        let node_b = DependencyNode {
            id: id_b,
            node_type: NodeType::Net { net_id: 99 },
            dependencies: vec![id_a],
            dependents: vec![id_a],
            dirty: false,
        };

        dag.nodes.insert(id_a, node_a);
        dag.nodes.insert(id_b, node_b);

        let result = dag.verify_no_cycles();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.cycle_nodes.len() >= 2);
    }

    #[test]
    fn test_verify_all_dependents_present_passes() {
        let mut dag = DependencyDag::new();
        dag.register_component(1, &[10]);
        dag.register_net(10, &[(0, 0)]);

        assert!(dag.verify_all_dependents_present().is_ok());
    }

    #[test]
    fn test_verify_all_dependents_present_detects_missing() {
        let mut dag = DependencyDag::new();
        let id_a = 100u64;
        let id_missing = 999u64;

        let node_a = DependencyNode {
            id: id_a,
            node_type: NodeType::Component { component_id: 1 },
            dependencies: vec![id_missing],
            dependents: Vec::new(),
            dirty: false,
        };

        dag.nodes.insert(id_a, node_a);

        let result = dag.verify_all_dependents_present();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.missing_ids.contains(&id_missing));
    }

    #[test]
    fn test_stats_returns_correct_counts() {
        let mut dag = DependencyDag::new();
        dag.register_component(1, &[10, 20]);
        dag.register_net(10, &[(0, 0)]);
        dag.register_net(20, &[(1, 1)]);

        dag.invalidate_component(1);

        let stats = dag.stats();
        // comp + 2 nets + 2 gcells = 5
        assert_eq!(stats.total_nodes, 5);
        assert!(stats.dirty_nodes >= 1); // component and possibly nets
        assert!(stats.total_edges > 0);
        assert!(stats.max_depth >= 1);
    }

    #[test]
    fn test_invalidate_net_marks_route_segments_dirty() {
        let mut dag = DependencyDag::new();
        dag.register_net(10, &[(0, 0)]);
        dag.register_route_segment(10, 0, &[], &[(0, 0)]);

        let dirty = dag.invalidate_net(10);
        assert!(!dirty.is_empty());

        let seg_id = DependencyDag::compute_node_id(&NodeType::RouteSegment { net_id: 10, segment_idx: 0 });
        assert!(dag.nodes.get(&seg_id).unwrap().dirty);
    }

    #[test]
    fn test_dirty_nets_and_gcells_unique() {
        let mut dag = DependencyDag::new();
        dag.register_component(1, &[10, 10]); // duplicate net ref
        dag.register_net(10, &[(0, 0), (0, 0)]); // duplicate gcell ref

        dag.invalidate_component(1);

        let nets = dag.get_dirty_nets();
        // net_id 10 should appear only once
        assert_eq!(nets.iter().filter(|&&n| n == 10).count(), 1);

        let gcells = dag.get_dirty_gcells();
        assert!(gcells.len() <= 1); // (0,0) at most once
    }
}
