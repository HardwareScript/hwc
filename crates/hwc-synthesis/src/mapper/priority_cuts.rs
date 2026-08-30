// crates/hwc-synthesis/src/mapper/priority_cuts.rs

use crate::aig::arena::{Edge, PackedAigGraph};
use crate::liberty::cell::StandardCell;
use crate::liberty::parser::LibertyCatalog;
use crate::mapper::npn::NpnCanonicalizer;
use compact_str::CompactString;
use smallvec::SmallVec;

pub const MAX_CUT_INPUTS: usize = 6;

/// A k-feasible cut of an AIG node.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorityCut {
    pub inputs: SmallVec<[u32; MAX_CUT_INPUTS]>,
    pub truth_table: u64,
    pub area_flow: f32,
    pub arrival_time_ps: f32,
}

/// A technology-mapped standard-cell instance.
#[derive(Debug, Clone)]
pub struct MappedInstance {
    pub node_id: u32,
    pub instance_name: CompactString,
    pub cell: StandardCell,
    pub input_nodes: Vec<u32>,
    pub output_node: u32,
}

pub struct PriorityCutMapper<'a> {
    pub graph: &'a PackedAigGraph,
    pub catalog: &'a LibertyCatalog,
}

impl<'a> PriorityCutMapper<'a> {
    pub fn new(graph: &'a PackedAigGraph, catalog: &'a LibertyCatalog) -> Self {
        Self { graph, catalog }
    }

    /// Computes technology mapping coverage for the AIG graph using standard cells.
    pub fn map_to_liberty(&self) -> Vec<MappedInstance> {
        let mut best_cuts: Vec<Option<PriorityCut>> = vec![None; self.graph.len()];
        let mut mapped_instances = Vec::new();
        let mut instance_counter = 0usize;

        // Initialize primary inputs
        for node_id in 1..self.graph.len() as u32 {
            if !self.graph.is_and(node_id) {
                let mut inputs = SmallVec::new();
                inputs.push(node_id);
                best_cuts[node_id as usize] = Some(PriorityCut {
                    inputs,
                    truth_table: 0xAAAA_AAAA_AAAA_AAAA,
                    area_flow: 0.0,
                    arrival_time_ps: 0.0,
                });
            }
        }

        // Process AND gates in topological order
        for node_id in 1..self.graph.len() as u32 {
            if !self.graph.is_and(node_id) {
                continue;
            }

            let (e0, e1) = self.graph.get_fanins(node_id);
            let cuts = self.enumerate_cuts(node_id, e0, e1, &best_cuts);

            let mut optimal_cut: Option<PriorityCut> = None;
            let mut best_cost = f32::MAX;
            let mut selected_cell: Option<StandardCell> = None;

            for cut in &cuts {
                let npn = NpnCanonicalizer::canonicalize(cut.truth_table, cut.inputs.len() as u8);

                // Match in Liberty catalog
                if let Some(cell) = self.catalog.get_by_npn(npn.canonical_tt) {
                    let cost = cut.arrival_time_ps + cell.delay_ps;
                    if cost < best_cost {
                        best_cost = cost;
                        optimal_cut = Some(cut.clone());
                        selected_cell = Some(cell.clone());
                    }
                } else if cut.inputs.len() == 2 {
                    // Fallback to NAND2 + INV decomposition if direct complex cell not matched
                    if let Some(nand_cell) = self.catalog.get_by_name("sky130_fd_sc_hd__nand2_1") {
                        let cost = cut.arrival_time_ps + nand_cell.delay_ps;
                        if cost < best_cost {
                            best_cost = cost;
                            optimal_cut = Some(cut.clone());
                            selected_cell = Some(nand_cell.clone());
                        }
                    }
                }
            }

            if let (Some(cut), Some(cell)) = (optimal_cut, selected_cell) {
                best_cuts[node_id as usize] = Some(cut.clone());
                instance_counter += 1;
                mapped_instances.push(MappedInstance {
                    node_id,
                    instance_name: CompactString::new(format!("gate_{}", instance_counter)),
                    cell,
                    input_nodes: cut.inputs.to_vec(),
                    output_node: node_id,
                });
            }
        }

        // Map sequential DFFs
        if let Some(dff_cell) = &self.catalog.dff_cell {
            for (idx, dff) in self.graph.registers.iter().enumerate() {
                mapped_instances.push(MappedInstance {
                    node_id: dff.q_output_node,
                    instance_name: CompactString::new(format!("dff_{}_{}", dff.name, idx)),
                    cell: dff_cell.clone(),
                    input_nodes: vec![dff.d_input.node()],
                    output_node: dff.q_output_node,
                });
            }
        }

        mapped_instances
    }

    fn enumerate_cuts(
        &self,
        _node_id: u32,
        e0: Edge,
        e1: Edge,
        best_cuts: &[Option<PriorityCut>],
    ) -> Vec<PriorityCut> {
        let mut cuts = Vec::new();

        // 1. Direct 2-input cut from immediate fanins
        let mut direct_inputs = SmallVec::new();
        direct_inputs.push(e0.node());
        if e0.node() != e1.node() {
            direct_inputs.push(e1.node());
        }

        let is_inv0 = e0.is_inverted();
        let is_inv1 = e1.is_inverted();
        let tt = match (is_inv0, is_inv1) {
            (false, false) => 0x8888_8888_8888_8888, // AND2
            (true, false) => 0x2222_2222_2222_2222,  // NOT A AND B
            (false, true) => 0x4444_4444_4444_4444,  // A AND NOT B
            (true, true) => 0x1111_1111_1111_1111,   // NOR2 (NOT A AND NOT B)
        };

        let t0 = best_cuts
            .get(e0.node() as usize)
            .and_then(|c| c.as_ref())
            .map_or(0.0, |c| c.arrival_time_ps);
        let t1 = best_cuts
            .get(e1.node() as usize)
            .and_then(|c| c.as_ref())
            .map_or(0.0, |c| c.arrival_time_ps);

        cuts.push(PriorityCut {
            inputs: direct_inputs,
            truth_table: tt,
            area_flow: 1.0,
            arrival_time_ps: t0.max(t1),
        });

        // 2. Multi-input cut expansion (up to K=6 inputs)
        if let (Some(Some(cut0)), Some(Some(cut1))) = (
            best_cuts.get(e0.node() as usize),
            best_cuts.get(e1.node() as usize),
        ) {
            let mut merged_inputs = cut0.inputs.clone();
            for &in1 in &cut1.inputs {
                if !merged_inputs.contains(&in1) {
                    merged_inputs.push(in1);
                }
            }

            if merged_inputs.len() <= MAX_CUT_INPUTS && merged_inputs.len() > 2 {
                cuts.push(PriorityCut {
                    inputs: merged_inputs,
                    truth_table: 0x1717_1717_1717_1717, // AOI / complex cut signature
                    area_flow: cut0.area_flow + cut1.area_flow + 1.0,
                    arrival_time_ps: cut0.arrival_time_ps.max(cut1.arrival_time_ps),
                });
            }
        }

        cuts
    }
}
