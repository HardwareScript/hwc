//! Netlist data types for SPICE export
//!
//! Shared types used by the modular netlist export system.

use rustc_hash::FxHashMap;

/// Parasitic element extracted from routing and layout geometry
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
    CouplingCapacitance {
        name: String,
        node_a: String,
        node_b: String,
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

    /// Retrieve the physical landing node corresponding to a top-level net
    pub fn get_top_level_pad_node(&self, net_name: &str) -> Option<&String> {
        self.net_entry_points.get(net_name)
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

use compact_str::CompactString;
use hwc_compiler::eval::MeasurementValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
    Inout,
    Power,
    Ground,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortInfo {
    pub name: CompactString,
    pub direction: PortDirection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhysicalDevice {
    pub name: CompactString,
    pub device_type: CompactString,
    pub device_type_id: usize,
    pub terminals: FxHashMap<CompactString, CompactString>,
    pub terminal_ports: FxHashMap<CompactString, CompactString>,
    pub terminal_layers: FxHashMap<CompactString, CompactString>,
    pub terminal_bindings: Vec<hwc_types::DeviceTerminalBinding>,
    pub params: FxHashMap<CompactString, MeasurementValue>,
    pub port_positions: FxHashMap<CompactString, (i64, i64)>,
    pub terminal_landings: Vec<hwc_engine::space::TerminalLanding>,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceTypeRegistry {
    pub types: Vec<CompactString>,
}

impl DeviceTypeRegistry {
    pub fn new() -> Self {
        Self { types: Vec::new() }
    }

    pub fn get_or_register(&mut self, name: &str) -> usize {
        if let Some(pos) = self.types.iter().position(|t| t.as_str() == name) {
            pos
        } else {
            let id = self.types.len();
            self.types.push(CompactString::new(name));
            id
        }
    }

    pub fn get_name(&self, id: usize) -> Option<&str> {
        self.types.get(id).map(|s| s.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct PhysicalNetlist {
    pub devices: Vec<PhysicalDevice>,
    pub device_registry: DeviceTypeRegistry,
    pub nets: FxHashMap<CompactString, Vec<CompactString>>,
}

impl PhysicalNetlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_emitter_records(records: &[hwc_compiler::eval::DeviceRecord]) -> Self {
        let mut netlist = Self::new();
        for rec in records {
            let type_id = netlist.device_registry.get_or_register(rec.device_type.as_str());
            let mut terms = FxHashMap::default();
            for (t_name, net_id) in &rec.terminals {
                terms.insert(t_name.clone(), CompactString::new(format!("net_{}", net_id.0)));
            }
            netlist.devices.push(PhysicalDevice {
                name: rec.name.clone(),
                device_type: rec.device_type.clone(),
                device_type_id: type_id,
                terminals: terms,
                terminal_ports: FxHashMap::default(),
                terminal_layers: FxHashMap::default(),
                terminal_bindings: Vec::new(),
                params: rec.params.clone(),
                port_positions: FxHashMap::default(),
                terminal_landings: Vec::new(),
            });
        }
        netlist
    }
}

