//! Netlist data types for SPICE export
//!
//! Shared types used by the modular netlist export system.

use rustc_hash::FxHashMap;

/// Parasitic element extracted from routing
#[derive(Debug, Clone)]
pub enum ParasiticElement {
    TraceResistor {
        name: String,
        node_a: String,
        node_b: String,
        value_ohms: f64,
    },
    GroundCapacitance {
        name: String,
        node: String,
        ref_node: String,
        value_farads: f64,
    },
}

/// Physical netlist graph with integrated parasitics
#[derive(Debug, Clone)]
pub struct PhysicalNetlistGraph {
    /// Device terminal connections (terminal -> physical node)
    pub device_nodes: FxHashMap<(String, String), String>, // (device_name, terminal) -> node_id
    /// Parasitic elements
    pub parasitics: Vec<ParasiticElement>,
    /// Net entry points (top-level net name -> entry node)
    pub net_entry_points: FxHashMap<String, String>,
}

impl PhysicalNetlistGraph {
    pub fn new() -> Self {
        Self {
            device_nodes: FxHashMap::default(),
            parasitics: Vec::new(),
            net_entry_points: FxHashMap::default(),
        }
    }
}

impl Default for PhysicalNetlistGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Stimulus generation mode for SPICE export
#[derive(Debug, Clone, Copy)]
pub enum StimulusMode {
    /// DC voltage sources with .op directive
    DcOperatingPoint,
    /// AC frequency sweep with .ac directive
    /// Policy-compliant expansion (v0.2.1): Separate file for frequency response
    AcFrequencyResponse,
    /// Pulsed voltage sources with .tran directive
    Transient,
}
