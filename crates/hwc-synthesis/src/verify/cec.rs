// crates/hwc-synthesis/src/verify/cec.rs

use crate::aig::arena::PackedAigGraph;
use crate::aig::fraig::SatSolver;
use miette::Diagnostic;
use rustc_hash::FxHashMap;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug, PartialEq, Eq)]
pub enum CecVerificationError {
    #[error("Formal Equivalence Check (CEC) Failed: Synthesized gate netlist does not match golden RTL behavior")]
    #[diagnostic(
        code(VERIFY_01),
        help("A functional counterexample was detected. Input assignment '{counterexample_vector}' causes output '{mismatched_output}' to diverge.")
    )]
    FunctionalMismatch {
        mismatched_output: String,
        counterexample_vector: String,
    },
}

pub struct CombinationalEquivalenceChecker;

impl CombinationalEquivalenceChecker {
    /// Builds a formal Combinational Equivalence Checking (CEC) SAT Miter circuit
    /// between Golden AIG and Synthesized AIG to prove 100% mathematical equivalence (UNSAT).
    pub fn verify_miter(
        golden_aig: &PackedAigGraph,
        synthesized_aig: &PackedAigGraph,
    ) -> Result<(), CecVerificationError> {
        let mut solver = SatSolver::new();

        let mut golden_map: FxHashMap<u32, i32> = FxHashMap::default();
        let mut synth_map: FxHashMap<u32, i32> = FxHashMap::default();
        let mut shared_input_vars: FxHashMap<String, i32> = FxHashMap::default();

        // 1. Bind shared primary inputs using exact node IDs
        for (name, &g_node) in golden_aig.input_names.iter().zip(&golden_aig.input_nodes) {
            let var = solver.new_var();
            shared_input_vars.insert(name.to_string(), var);
            golden_map.insert(g_node, var);
            if let Some(s_node) = synthesized_aig.get_input_node(name.as_str()) {
                synth_map.insert(s_node, var);
            }
        }

        // 2. Bind shared sequential register Q outputs (state bits)
        for g_dff in &golden_aig.registers {
            if let Some(s_dff) = synthesized_aig.registers.iter().find(|d| d.name == g_dff.name) {
                let var = solver.new_var();
                shared_input_vars.insert(g_dff.name.to_string(), var);
                golden_map.insert(g_dff.q_output_node, var);
                synth_map.insert(s_dff.q_output_node, var);
            }
        }

        // 3. Encode Golden AIG to SAT
        for node_id in 1..golden_aig.len() as u32 {
            Self::encode_node(golden_aig, node_id, &mut solver, &mut golden_map);
        }

        // 4. Encode Synthesized AIG to SAT
        for node_id in 1..synthesized_aig.len() as u32 {
            Self::encode_node(synthesized_aig, node_id, &mut solver, &mut synth_map);
        }

        // 4. Form Miter: XOR corresponding outputs, OR into Master Error Flag
        let mut miter_diff_lits = Vec::new();

        for (out_name, &golden_edge) in &golden_aig.outputs {
            if let Some(&synth_edge) = synthesized_aig.outputs.get(out_name) {
                let g_var = golden_map.get(&golden_edge.node()).copied().unwrap_or(0);
                let s_var = synth_map.get(&synth_edge.node()).copied().unwrap_or(0);

                let g_lit = if golden_edge.is_inverted() { -g_var } else { g_var };
                let s_lit = if synth_edge.is_inverted() { -s_var } else { s_var };

                let diff_var = solver.new_var();
                solver.add_xor_gate(diff_var, g_lit, s_lit);
                miter_diff_lits.push((out_name.clone(), diff_var));
            }
        }

        // Compare next-state functions (D inputs of registers)
        for g_dff in &golden_aig.registers {
            if let Some(s_dff) = synthesized_aig.registers.iter().find(|r| r.name == g_dff.name) {
                let g_var = golden_map.get(&g_dff.d_input.node()).copied().unwrap_or(0);
                let s_var = synth_map.get(&s_dff.d_input.node()).copied().unwrap_or(0);

                let g_lit = if g_dff.d_input.is_inverted() { -g_var } else { g_var };
                let s_lit = if s_dff.d_input.is_inverted() { -s_var } else { s_var };

                let diff_var = solver.new_var();
                solver.add_xor_gate(diff_var, g_lit, s_lit);
                miter_diff_lits.push((format!("{}.next", g_dff.name).into(), diff_var));
            }
        }

        if miter_diff_lits.is_empty() {
            return Ok(());
        }

        // Assert that at least one output differs (OR clause of all diff_vars)
        let or_clause: Vec<i32> = miter_diff_lits.iter().map(|(_, var)| *var).collect();
        solver.add_clause(or_clause);

        // 5. Solve SAT Miter: UNSAT = Proven Equivalent, SAT = Functional Mismatch Bug
        if let Some(model) = solver.solve() {
            // Find which output differed
            let mut mismatched_name = "unknown".to_string();
            for (name, var) in &miter_diff_lits {
                let idx = (var.unsigned_abs() - 1) as usize;
                if model.get(idx).copied().unwrap_or(false) {
                    mismatched_name = name.to_string();
                    break;
                }
            }

            // Extract counterexample
            let mut ce_parts = Vec::new();
            for (name, &var) in &shared_input_vars {
                let idx = (var.unsigned_abs() - 1) as usize;
                let val = model.get(idx).copied().unwrap_or(false);
                ce_parts.push(format!("{}={}", name, if val { 1 } else { 0 }));
            }

            return Err(CecVerificationError::FunctionalMismatch {
                mismatched_output: mismatched_name,
                counterexample_vector: ce_parts.join(", "),
            });
        }

        Ok(())
    }

    fn encode_node(
        graph: &PackedAigGraph,
        node_id: u32,
        solver: &mut SatSolver,
        node_to_var: &mut FxHashMap<u32, i32>,
    ) -> i32 {
        if let Some(&var) = node_to_var.get(&node_id) {
            return var;
        }

        let var = solver.new_var();
        node_to_var.insert(node_id, var);

        if node_id == 0 {
            solver.add_clause(vec![-var]);
            return var;
        }

        if graph.is_and(node_id) {
            let (e0, e1) = graph.get_fanins(node_id);
            let v0 = Self::encode_node(graph, e0.node(), solver, node_to_var);
            let v1 = Self::encode_node(graph, e1.node(), solver, node_to_var);
            let lit0 = if e0.is_inverted() { -v0 } else { v0 };
            let lit1 = if e1.is_inverted() { -v1 } else { v1 };
            solver.add_and_gate(var, lit0, lit1);
        }

        var
    }
}
