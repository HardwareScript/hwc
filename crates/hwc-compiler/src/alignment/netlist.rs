//! Netlist Data Structures for Alignment Validation
//!
//! This module defines the common netlist representation used for comparing
//! physical geometry (extracted from `space`) against logical intent (from `module`).
//!
//! # Design Philosophy
//!
//! Both physical and logical netlists use the same data structures, making
//! comparison straightforward. The only difference is their source:
//! - **Physical Netlist**: Extracted from physical geometry by Device Extractor
//! - **Logical Netlist**: Synthesized from module definition by Logical Synthesizer
//!
//! # Data-Driven Architecture
//!
//! Following the Hardware Script philosophy, device types are NOT hardcoded.
//! Instead, they are dynamically registered in a DeviceTypeRegistry, just like
//! materials in MaterialRegistry. This allows users to define custom device types
//! without modifying the compiler.

use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Device type ID (dynamically registered, not hardcoded)
pub type DeviceTypeId = u16;

/// Device type registry - dynamically registers device types
///
/// This follows the same pattern as MaterialRegistry - device types are
/// registered on-demand from .hw files, not hardcoded into the compiler.
#[derive(Debug, Clone)]
pub struct DeviceTypeRegistry {
    /// Fast lookup: Name → ID
    name_to_id: FxHashMap<CompactString, DeviceTypeId>,
    /// Fast lookup: ID → Name (Vec for O(1) indexing)
    id_to_name: Vec<CompactString>,
}

impl DeviceTypeRegistry {
    /// Create a new device type registry
    pub fn new() -> Self {
        Self {
            name_to_id: FxHashMap::default(),
            id_to_name: Vec::new(),
        }
    }

    /// Get or register a device type and return its ID
    ///
    /// # Arguments
    /// * `name` - Device type name from .hw file (e.g., "NMOS", "PMOS", "BJT")
    ///
    /// # Returns
    /// Device type ID
    pub fn get_or_register(&mut self, name: &str) -> DeviceTypeId {
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }

        let id = self.id_to_name.len() as DeviceTypeId;
        self.id_to_name.push(name.into());
        self.name_to_id.insert(name.into(), id);
        id
    }

    /// Get device type ID by name
    pub fn get_id(&self, name: &str) -> Option<DeviceTypeId> {
        self.name_to_id.get(name).copied()
    }

    /// Get device type name by ID
    pub fn get_name(&self, id: DeviceTypeId) -> Option<&str> {
        self.id_to_name.get(id as usize).map(|s| s.as_str())
    }

    /// Get all registered device types
    pub fn all_types(&self) -> Vec<(DeviceTypeId, &str)> {
        self.id_to_name
            .iter()
            .enumerate()
            .map(|(id, name)| (id as DeviceTypeId, name.as_str()))
            .collect()
    }
}

impl Default for DeviceTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Port direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
    Inout,
    Power,
    Ground,
}

impl std::fmt::Display for PortDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortDirection::Input => write!(f, "input"),
            PortDirection::Output => write!(f, "output"),
            PortDirection::Inout => write!(f, "inout"),
            PortDirection::Power => write!(f, "power"),
            PortDirection::Ground => write!(f, "ground"),
        }
    }
}

/// Port information
#[derive(Debug, Clone)]
pub struct PortInfo {
    pub name: CompactString,
    pub direction: PortDirection,
}

/// Net information
#[derive(Debug, Clone)]
pub struct NetInfo {
    pub name: CompactString,
    pub connected_devices: Vec<CompactString>,
}

impl NetInfo {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string().into(),
            connected_devices: Vec::new(),
        }
    }
}

/// Physical netlist extracted from geometry
#[derive(Debug, Clone)]
pub struct PhysicalNetlist {
    pub devices: Vec<PhysicalDevice>,
    pub nets: FxHashMap<CompactString, NetInfo>,
    pub ports: Vec<PortInfo>,
    pub device_registry: DeviceTypeRegistry, // Registry for device type name lookup
}

impl PhysicalNetlist {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            nets: FxHashMap::default(),
            ports: Vec::new(),
            device_registry: DeviceTypeRegistry::new(),
        }
    }

    /// Create a new PhysicalNetlist with a specific device registry
    pub fn with_registry(device_registry: DeviceTypeRegistry) -> Self {
        Self {
            devices: Vec::new(),
            nets: FxHashMap::default(),
            ports: Vec::new(),
            device_registry,
        }
    }
}

impl Default for PhysicalNetlist {
    fn default() -> Self {
        Self::new()
    }
}

/// Physical device extracted from geometry
#[derive(Debug, Clone)]
pub struct PhysicalDevice {
    pub name: CompactString,
    pub device_type_id: DeviceTypeId, // Dynamic ID, not hardcoded enum
    pub terminals: FxHashMap<CompactString, String>, // terminal_name -> net_name
    pub parameters: FxHashMap<CompactString, hwc_engine::PhysicalQuantity>, // Strongly-typed W, L, AS, AD, etc.
    /// Pour names for each terminal (for spatial error reporting)
    pub terminal_pours: FxHashMap<CompactString, String>, // terminal_name -> pour_name
}

/// Logical netlist synthesized from module definition
#[derive(Debug, Clone)]
pub struct LogicalNetlist {
    pub devices: Vec<LogicalDevice>,
    pub nets: FxHashMap<CompactString, NetInfo>,
    pub ports: Vec<PortInfo>,
}

impl LogicalNetlist {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            nets: FxHashMap::default(),
            ports: Vec::new(),
        }
    }
}

impl Default for LogicalNetlist {
    fn default() -> Self {
        Self::new()
    }
}

/// Logical device from module definition
#[derive(Debug, Clone)]
pub struct LogicalDevice {
    pub name: CompactString,
    pub device_type_id: DeviceTypeId, // Dynamic ID, not hardcoded enum
    pub terminals: FxHashMap<CompactString, String>, // terminal_name -> net_name
    pub parameters: FxHashMap<CompactString, f64>, // W, L (if specified)
}

/// Graph representation of a netlist for isomorphism checking
#[derive(Debug, Clone)]
pub struct NetlistGraph {
    pub nodes: FxHashMap<CompactString, NetNode>, // net_name -> node
    pub edges: Vec<DeviceEdge>,                   // device connections
}

impl NetlistGraph {
    pub fn new() -> Self {
        Self {
            nodes: FxHashMap::default(),
            edges: Vec::new(),
        }
    }
}

impl Default for NetlistGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Net node in the graph
#[derive(Debug, Clone)]
pub struct NetNode {
    pub net_name: CompactString,
    pub connected_devices: Vec<CompactString>,
}

impl NetNode {
    pub fn new(net_name: &str) -> Self {
        Self {
            net_name: net_name.to_string().into(),
            connected_devices: Vec::new(),
        }
    }
}

/// Device edge in the graph
#[derive(Debug, Clone)]
pub struct DeviceEdge {
    pub device_name: CompactString,
    pub device_type_id: DeviceTypeId, // Dynamic ID, not hardcoded enum
    pub connections: FxHashMap<CompactString, String>, // terminal -> net
}
