// crates/hwc-synthesis/src/aig/fraig.rs

use crate::aig::arena::{Edge, PackedAigGraph};
use rustc_hash::FxHashMap;

/// High-performance embedded SAT solver for FRAIG sweeping and Equivalence Miters.
#[derive(Debug, Clone, Default)]
pub struct SatSolver {
    num_vars: usize,
    clauses: Vec<Vec<i32>>,
}

impl SatSolver {
    pub fn new() -> Self {
        Self {
            num_vars: 0,
            clauses: Vec::new(),
        }
    }

    pub fn new_var(&mut self) -> i32 {
        self.num_vars += 1;
        self.num_vars as i32
    }

    pub fn add_clause(&mut self, clause: Vec<i32>) {
        self.clauses.push(clause);
    }

    /// Add CNF clauses encoding: out <=> (in0 & in1)
    pub fn add_and_gate(&mut self, out: i32, in0: i32, in1: i32) {
        // (NOT out OR in0)
        self.add_clause(vec![-out, in0]);
        // (NOT out OR in1)
        self.add_clause(vec![-out, in1]);
        // (NOT in0 OR NOT in1 OR out)
        self.add_clause(vec![-in0, -in1, out]);
    }

    /// Add CNF clauses encoding: out <=> (in0 ^ in1)
    pub fn add_xor_gate(&mut self, out: i32, in0: i32, in1: i32) {
        // (NOT in0 OR NOT in1 OR NOT out)
        self.add_clause(vec![-in0, -in1, -out]);
        // (in0 OR in1 OR NOT out)
        self.add_clause(vec![in0, in1, -out]);
        // (NOT in0 OR in1 OR out)
        self.add_clause(vec![-in0, in1, out]);
        // (in0 OR NOT in1 OR out)
        self.add_clause(vec![in0, -in1, out]);
    }

    /// Solves the CNF formula via DPLL with unit propagation and clean backtracking.
    pub fn solve(&self) -> Option<Vec<bool>> {
        let mut assignment = vec![None; self.num_vars + 1];
        if self.dpll(&mut assignment) {
            Some(
                assignment
                    .into_iter()
                    .skip(1)
                    .map(|v| v.unwrap_or(false))
                    .collect(),
            )
        } else {
            None
        }
    }

    fn dpll(&self, assignment: &mut [Option<bool>]) -> bool {
        let mut snapshot = assignment.to_vec();
        if !self.unit_propagate(&mut snapshot) {
            return false;
        }

        let next_var = snapshot
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, val)| val.is_none())
            .map(|(idx, _)| idx);

        let Some(var) = next_var else {
            assignment.copy_from_slice(&snapshot);
            return true;
        };

        // Try assigning true
        let mut branch_true = snapshot.clone();
        branch_true[var] = Some(true);
        if self.dpll(&mut branch_true) {
            assignment.copy_from_slice(&branch_true);
            return true;
        }

        // Try assigning false
        let mut branch_false = snapshot;
        branch_false[var] = Some(false);
        if self.dpll(&mut branch_false) {
            assignment.copy_from_slice(&branch_false);
            return true;
        }

        false
    }

    fn unit_propagate(&self, assignment: &mut [Option<bool>]) -> bool {
        let mut changed = true;
        while changed {
            changed = false;
            for clause in &self.clauses {
                let mut unassigned_lit = None;
                let mut satisfied = false;
                let mut open_lits = 0;

                for &lit in clause {
                    let var = lit.unsigned_abs() as usize;
                    let sign = lit > 0;
                    match assignment.get(var) {
                        Some(&Some(val)) => {
                            if val == sign {
                                satisfied = true;
                                break;
                            }
                        }
                        Some(&None) => {
                            open_lits += 1;
                            unassigned_lit = Some(lit);
                        }
                        None => {}
                    }
                }

                if satisfied {
                    continue;
                }

                if open_lits == 0 {
                    return false; // Conflict
                }

                if open_lits == 1 {
                    let lit = unassigned_lit.unwrap_or(0);
                    let var = lit.unsigned_abs() as usize;
                    let sign = lit > 0;
                    if let Some(slot) = assignment.get_mut(var) {
                        *slot = Some(sign);
                        changed = true;
                    }
                }
            }
        }
        true
    }
}

/// Functionally Reduced And-Inverter Graph (FRAIG) optimization engine.
pub struct FraigOptimizer;

impl FraigOptimizer {
    /// Perform SIMD bit-parallel simulation (64-wide) and SAT sweeping to merge functionally equivalent nodes.
    pub fn optimize(graph: &PackedAigGraph) -> (PackedAigGraph, usize) {
        if graph.len() <= 1 {
            return (graph.clone(), 0);
        }

        let mut sim_patterns: Vec<u64> = vec![0; graph.len()];
        // Constant 0 produces 0
        sim_patterns[0] = 0;

        // Seed primary inputs with deterministic pseudo-random bit patterns
        let mut lcg: u64 = 0xdead_beef_cafe_babe;
        for node_id in 1..graph.len() {
            if !graph.is_and(node_id as u32) {
                // LCG step
                lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                sim_patterns[node_id] = lcg;
            }
        }

        // Simulate AND nodes in topological order
        for node_id in 1..graph.len() {
            if graph.is_and(node_id as u32) {
                let (e0, e1) = graph.get_fanins(node_id as u32);
                let p0 = if e0.is_inverted() {
                    !sim_patterns[e0.node() as usize]
                } else {
                    sim_patterns[e0.node() as usize]
                };
                let p1 = if e1.is_inverted() {
                    !sim_patterns[e1.node() as usize]
                } else {
                    sim_patterns[e1.node() as usize]
                };
                sim_patterns[node_id] = p0 & p1;
            }
        }

        // Partition nodes into candidate equivalence classes based on simulation signature
        let mut sim_map: FxHashMap<u64, u32> = FxHashMap::default();
        let mut replacements: FxHashMap<u32, Edge> = FxHashMap::default();
        let mut merged_count = 0;

        for node_id in 1..graph.len() as u32 {
            let pattern = sim_patterns[node_id as usize];
            let norm_pattern = pattern.min(!pattern);
            let is_inverted = pattern > !pattern;

            if let Some(&leader_node) = sim_map.get(&norm_pattern) {
                // Candidate match: verify SAT equivalence
                if Self::prove_equivalence(graph, node_id, leader_node, is_inverted) {
                    replacements.insert(node_id, Edge::new(leader_node, is_inverted));
                    merged_count += 1;
                    continue;
                }
            }
            sim_map.insert(norm_pattern, node_id);
        }

        // Reconstruct optimized AIG graph
        let mut optimized = PackedAigGraph::with_capacity(graph.len() - merged_count);
        let mut node_mapping: Vec<Edge> = vec![Edge::ZERO; graph.len()];

        // 1. Copy primary inputs using their exact original node IDs
        for (name, &orig_node) in graph.input_names.iter().zip(&graph.input_nodes) {
            let new_edge = optimized.add_input(name);
            node_mapping[orig_node as usize] = new_edge;
        }

        // 2. Copy DFF Q pseudo-inputs
        for dff in &graph.registers {
            let new_q_edge = optimized.add_dff(
                &dff.name,
                Edge::ZERO,
                &dff.clock_signal,
                dff.reset_signal.as_deref(),
                dff.reset_value,
            );
            node_mapping[dff.q_output_node as usize] = new_q_edge;
        }

        // 3. Rebuild AND gates with replacements applied
        for node_id in 1..graph.len() as u32 {
            if !graph.is_and(node_id) {
                continue;
            }

            if let Some(&repl) = replacements.get(&node_id) {
                let target_edge = node_mapping[repl.node() as usize];
                node_mapping[node_id as usize] = if repl.is_inverted() {
                    target_edge.not()
                } else {
                    target_edge
                };
                continue;
            }

            let (e0, e1) = graph.get_fanins(node_id);
            let mapped_e0 = Self::resolve_edge(e0, &node_mapping);
            let mapped_e1 = Self::resolve_edge(e1, &node_mapping);

            let new_edge = optimized.add_and(mapped_e0, mapped_e1);
            node_mapping[node_id as usize] = new_edge;
        }

        // 4. Remap DFF D inputs
        for (idx, dff) in graph.registers.iter().enumerate() {
            let mapped_d = Self::resolve_edge(dff.d_input, &node_mapping);
            optimized.registers[idx].d_input = mapped_d;
        }

        // 5. Remap outputs
        for (name, &orig_edge) in &graph.outputs {
            let mapped_out = Self::resolve_edge(orig_edge, &node_mapping);
            optimized.set_output(name, mapped_out);
        }

        (optimized, merged_count)
    }

    #[inline]
    fn resolve_edge(edge: Edge, node_mapping: &[Edge]) -> Edge {
        if edge.node() == 0 {
            return edge;
        }
        let base = node_mapping[edge.node() as usize];
        if edge.is_inverted() {
            base.not()
        } else {
            base
        }
    }

    /// Prove whether node_a <=> (node_b ^ inverted) by constructing a SAT miter.
    fn prove_equivalence(
        graph: &PackedAigGraph,
        node_a: u32,
        node_b: u32,
        is_inverted: bool,
    ) -> bool {
        let mut solver = SatSolver::new();
        let mut node_to_var: FxHashMap<u32, i32> = FxHashMap::default();

        // Encode cone of node_a and node_b
        let var_a = Self::encode_cone_to_sat(graph, node_a, &mut solver, &mut node_to_var);
        let var_b = Self::encode_cone_to_sat(graph, node_b, &mut solver, &mut node_to_var);

        // Miter target: var_a != (var_b ^ inverted)
        // If inverted: prove NOT(var_a == NOT var_b) -> var_a == var_b must be UNSAT
        // If not inverted: prove NOT(var_a == var_b) -> var_a != var_b must be UNSAT
        let target_var_b = if is_inverted { -var_b } else { var_b };
        let diff_var = solver.new_var();
        solver.add_xor_gate(diff_var, var_a, target_var_b);
        solver.add_clause(vec![diff_var]); // Assert difference = 1

        // If UNSAT, they are functionally identical!
        solver.solve().is_none()
    }

    fn encode_cone_to_sat(
        graph: &PackedAigGraph,
        root: u32,
        solver: &mut SatSolver,
        node_to_var: &mut FxHashMap<u32, i32>,
    ) -> i32 {
        if let Some(&var) = node_to_var.get(&root) {
            return var;
        }

        let var = solver.new_var();
        node_to_var.insert(root, var);

        if root == 0 {
            solver.add_clause(vec![-var]);
            return var;
        }

        if graph.is_and(root) {
            let (e0, e1) = graph.get_fanins(root);
            let v0 = Self::encode_cone_to_sat(graph, e0.node(), solver, node_to_var);
            let v1 = Self::encode_cone_to_sat(graph, e1.node(), solver, node_to_var);
            let lit0 = if e0.is_inverted() { -v0 } else { v0 };
            let lit1 = if e1.is_inverted() { -v1 } else { v1 };
            solver.add_and_gate(var, lit0, lit1);
        }

        var
    }
}
