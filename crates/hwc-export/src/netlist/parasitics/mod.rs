//! Modular Parasitic Extraction Engine for SPICE Export.
//!
//! Translates 3D layout geometries, via columns, and interconnect routes into
//! an exact physical SPICE sub-circuit network.
//!
//! # Architecture Pipeline:
//! 1. `via_stacks`: Spatial via clustering, channel contact exemption, and parallel resistance.
//! 2. `routes`: Trace series resistance and microstrip ground capacitance.
//! 3. `coupling`: 2.5D lateral sidewall coupling capacitance between parallel traces.
//! 4. `pours`: Conductive interconnect bus mesh resistance (Rbus) and substrate capacitance.
//! 5. `terminals`: Intent-driven mapping of device terminals to physical interface nodes.

pub mod coupling;
pub mod geometry;
pub mod pours;
pub mod routes;
pub mod terminals;
pub mod types;
pub mod via_stacks;

use rustc_hash::FxHashMap;

use self::coupling::extract_lateral_coupling;
use self::pours::extract_interconnect_pours;
use self::routes::extract_traces;
use self::terminals::map_device_terminals;
use self::types::ExtractedClusterNode;
use self::via_stacks::extract_via_stacks;

use crate::netlist::types::{PhysicalNetlist, PhysicalNetlistGraph};
use hwc_compiler::SymbolTable;
use hwc_engine::HardwareSpace;

/// Core parasitic extraction orchestrator.
pub struct ParasiticExtractor<'a> {
    space: &'a HardwareSpace,
    symbol_table: &'a SymbolTable,
    physical_netlist: Option<&'a PhysicalNetlist>,
    substrate_net: String,
    graph: PhysicalNetlistGraph,
    extracted_layer_nodes: FxHashMap<(String, String), Vec<ExtractedClusterNode>>,
}

impl<'a> ParasiticExtractor<'a> {
    /// Create a new ParasiticExtractor instance.
    pub fn new(
        space: &'a HardwareSpace,
        symbol_table: &'a SymbolTable,
        physical_netlist: Option<&'a PhysicalNetlist>,
        substrate_net: &str,
    ) -> Self {
        Self {
            space,
            symbol_table,
            physical_netlist,
            substrate_net: substrate_net.to_string(),
            graph: PhysicalNetlistGraph::new(),
            extracted_layer_nodes: FxHashMap::default(),
        }
    }

    /// Run the multi-stage extraction pipeline and return the completed PhysicalNetlistGraph.
    pub fn extract(mut self) -> Result<PhysicalNetlistGraph, Box<dyn std::error::Error>> {
        eprintln!("[PARASITIC EXTRACTION] Starting extraction pipeline...");
        
        // Stage 1: Spatial Via Stack Clustering & Parallel Resistance
        eprintln!("[PARASITIC EXTRACTION] Stage 1: Via stacks");
        extract_via_stacks(
            self.space,
            self.physical_netlist,
            &mut self.graph,
            &mut self.extracted_layer_nodes,
        );
        eprintln!("[PARASITIC EXTRACTION] Stage 1 complete: {} parasitics, {} layer nodes",
            self.graph.parasitics.len(), self.extracted_layer_nodes.len());

        // Stage 2: Series Trace Resistance & Microstrip Ground Capacitance
        eprintln!("[PARASITIC EXTRACTION] Stage 2: Traces");
        extract_traces(
            self.space,
            &self.substrate_net,
            &mut self.graph,
            &mut self.extracted_layer_nodes,
        );
        eprintln!("[PARASITIC EXTRACTION] Stage 2 complete: {} parasitics",
            self.graph.parasitics.len());

        // Stage 3: 2.5D Lateral Coupling Capacitance
        eprintln!("[PARASITIC EXTRACTION] Stage 3: Coupling");
        extract_lateral_coupling(
            self.space,
            &mut self.graph,
            &self.extracted_layer_nodes,
        );
        eprintln!("[PARASITIC EXTRACTION] Stage 3 complete: {} parasitics",
            self.graph.parasitics.len());

        // Stage 4: Conductive Bus Mesh Resistance & Substrate Capacitance
        eprintln!("[PARASITIC EXTRACTION] Stage 4: Interconnect pours");
        extract_interconnect_pours(
            self.space,
            &self.substrate_net,
            &mut self.graph,
            &self.extracted_layer_nodes,
        );
        eprintln!("[PARASITIC EXTRACTION] Stage 4 complete: {} parasitics",
            self.graph.parasitics.len());

        // Stage 5: Intent-Driven Device Terminal Mapping
        eprintln!("[PARASITIC EXTRACTION] Stage 5: Device terminal mapping");
        map_device_terminals(
            self.space,
            self.symbol_table,
            self.physical_netlist,
            &mut self.graph,
            &self.extracted_layer_nodes,
        );
        eprintln!("[PARASITIC EXTRACTION] Stage 5 complete");
        eprintln!("[PARASITIC EXTRACTION] Final: {} parasitics, {} device nodes, {} net entry points",
            self.graph.parasitics.len(), self.graph.device_nodes.len(), self.graph.net_entry_points.len());

        Ok(self.graph)
    }
}

/// Public entry point for parasitic extraction into a PhysicalNetlistGraph.
pub fn build_physical_netlist_graph(
    space: &HardwareSpace,
    symbol_table: &hwc_compiler::SymbolTable,
    physical_netlist: Option<&PhysicalNetlist>,
    substrate_net: &str,
) -> Result<PhysicalNetlistGraph, Box<dyn std::error::Error>> {
    let extractor = ParasiticExtractor::new(space, symbol_table, physical_netlist, substrate_net);
    extractor.extract()
}
