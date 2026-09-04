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

pub mod boundary_bridges;
pub mod coupling;
pub mod geometry;
pub mod pours;
pub mod routes;
pub mod terminals;
pub mod types;
pub mod via_stacks;

use rustc_hash::FxHashMap;

use self::boundary_bridges::emit_boundary_pad_bridges;
use self::coupling::extract_lateral_coupling;
use self::pours::extract_interconnect_pours;
use self::routes::extract_traces;
use self::terminals::map_device_terminals;
use self::types::{ExtractedClusterNode, ExtractionConfig};
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
    config: ExtractionConfig,
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
        let is_asic = space.technology_strategy.is_asic()
            || space.fabrication_constraints.as_ref().is_some_and(|c| c.technology.is_asic());

        let profile = symbol_table
            .get_space(space.name.as_str())
            .ok()
            .and_then(|space_decl| space_decl.profile.as_ref())
            .and_then(|pname| symbol_table.get_profile(pname.name.as_str()).ok())
            .or_else(|| symbol_table.profiles().map(|(_, p)| p).next());

        let config = ExtractionConfig::from_profile(profile, is_asic);

        Self {
            space,
            symbol_table,
            physical_netlist,
            substrate_net: substrate_net.to_string(),
            config,
            graph: PhysicalNetlistGraph::new(),
            extracted_layer_nodes: FxHashMap::default(),
        }
    }

    /// Run the multi-stage extraction pipeline and return the completed PhysicalNetlistGraph.
    pub fn extract(mut self) -> Result<PhysicalNetlistGraph, Box<dyn std::error::Error>> {
                
        // Stage 1: Spatial Via Stack Clustering & Parallel Resistance
        extract_via_stacks(
            self.space,
            self.physical_netlist,
            &mut self.graph,
            &mut self.extracted_layer_nodes,
            &self.config,
        );
        
        // Stage 2: Series Trace Resistance & Microstrip Ground Capacitance
        extract_traces(
            self.space,
            &self.substrate_net,
            &mut self.graph,
            &mut self.extracted_layer_nodes,
            &self.config,
        );
        
        // Stage 3: 2.5D Lateral Coupling Capacitance
        extract_lateral_coupling(
            self.space,
            &mut self.graph,
            &self.extracted_layer_nodes,
            &self.config,
        );
        
        // Stage 4: Conductive Bus Mesh Resistance & Substrate Capacitance
        extract_interconnect_pours(
            self.space,
            &self.substrate_net,
            &mut self.graph,
            &self.extracted_layer_nodes,
        );
        
        // Stage 5: Intent-Driven Device Terminal Mapping
        map_device_terminals(
            self.space,
            self.symbol_table,
            self.physical_netlist,
            &mut self.graph,
            &self.extracted_layer_nodes,
        );

        // Stage 6: Boundary Net Bridging (Stimulus Nets → n{Net}_pad nodes)
        // Zero string-scraping: bridge is built from typed ExternalPad pour records.
        emit_boundary_pad_bridges(
            self.space,
            &mut self.graph,
        );
                
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
