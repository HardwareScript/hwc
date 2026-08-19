use crate::geometry::BoundingBox;
use crate::netlist::NetId;
use compact_str::CompactString;

/// **v0.1.7: Keep-Out Zone (DRC & Auto-Placement Level)**
///
/// Defines a region where certain layout features (vias, traces, components)
/// are forbidden to ensure mechanical and electrical integrity.
#[derive(Debug, Clone)]
pub struct KeepOutZone {
    pub bbox: BoundingBox,
    /// If Some, this net is exempt from this keep-out zone (allows its own traces/vias)
    pub net_id: Option<NetId>,
    /// If false, automatic via insertion is forbidden in this zone
    pub allow_vias: bool,
    /// If false, signal routing is forbidden in this zone
    pub allow_routing: bool,
    /// List of net names that are exempt from this keep-out zone (v0.1.7)
    pub exempted_nets: Vec<CompactString>,
}

/// Metadata about a material pour for engineering artifacts
///
/// Phase 4 (Silent Atom): Added device_binding field for explicit intent-based extraction
#[derive(Debug, Clone)]
pub struct PourMetadata {
    pub name: CompactString,
    pub material_name: CompactString,
    /// Stackup layer name where the pour resides
    pub layer_name: CompactString,
    /// Bottom Z elevation of the pour in nanometers (v0.1.7 physical truth).
    pub z_bottom_nm: i64,
    pub net: Option<CompactString>,
    pub area_nm2: i64,
    /// Bounding box in nanometers (for geometric overlap detection)
    pub bbox: Option<crate::geometry::BoundingBox>,
    /// Phase 4: Explicit device terminal binding (e.g., "M1.gate")
    pub device_binding: Option<DeviceBinding>,
    /// Sprint 3.2: Merged region tracking for parasitic extraction
    pub merged_region_id: Option<CompactString>,
    /// v0.1.7: Intentional design waivers (Silicon Law)
    pub waivers: hwc_parser::Waivers,
}

/// Device binding for explicit intent-based extraction (Phase 4: Silent Atom)
///
/// Binding priority for device terminal assignments (v0.2.2)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindingPriority {
    Channel = 0,
    Contact = 100,
}

impl Default for BindingPriority {
    fn default() -> Self {
        Self::Contact
    }
}

impl From<hwc_parser::BindingPriority> for BindingPriority {
    fn from(parser_priority: hwc_parser::BindingPriority) -> Self {
        match parser_priority {
            hwc_parser::BindingPriority::Channel => Self::Channel,
            hwc_parser::BindingPriority::Contact => Self::Contact,
        }
    }
}

/// Binds a pour to device terminal(s), eliminating geometric guessing.
/// v0.2.2: Supports multi-terminal binding (e.g., R1.A and R1.B on same pour) with priority
#[derive(Debug, Clone)]
pub struct DeviceBinding {
    pub device_name: CompactString, // e.g., "M1", "R1"
    pub terminals: Vec<CompactString>, // e.g., ["gate"], or ["A", "B"] for resistors
    pub priority: BindingPriority, // v0.2.2: Processing order priority
}

/// Device instance metadata (v0.2.1: Native Device Support)
///
/// Stores information about a device instance discovered from pour bindings.
/// This is the single source of truth for device extraction to SPICE, BOM, etc.
#[derive(Debug, Clone)]
pub struct DeviceInstance {
    /// Instance name (e.g., "R1", "M1")
    pub name: CompactString,
    /// Device type name (e.g., "Resistor", "NMOS", "Capacitor")
    pub device_type: CompactString,
    /// Terminal names (e.g., ["A", "B"] for resistor, ["gate", "source", "drain", "bulk"] for MOSFET)
    pub terminals: Vec<CompactString>,
    /// Net connections for each terminal (terminal_name -> net_name)
    pub terminal_nets: rustc_hash::FxHashMap<CompactString, CompactString>,
    /// Calculated parameters (e.g., "R" -> 400.0 for resistance, "W" -> 1.0, "L" -> 0.18 for transistors)
    pub parameters: rustc_hash::FxHashMap<CompactString, f64>,
}

/// Metadata about a contact/via for connectivity checking
#[derive(Debug, Clone)]
pub struct ContactMetadata {
    pub name: CompactString,
    pub material_name: CompactString,
    /// Bottom Z of the lower connected pour plane in nanometers.
    pub z_start_nm: i64,
    /// Bottom Z of the upper connected pour plane in nanometers.
    pub z_end_nm: i64,
    pub net: Option<CompactString>,
    pub bridge: Option<CompactString>,
    pub bbox: Option<crate::geometry::BoundingBox>,
    /// Actual via drill diameter in nanometers (excludes annular ring/pad extension).
    /// The bbox includes pad (drill + 2*enclosure); this is the inner hole only.
    pub drill_diameter_nm: Option<i64>,
    /// Whether the via is tented (covered by solder mask) — v0.1.7
    pub is_tented: bool,
    /// Optional explicit solder mask opening diameter in nanometers — v0.1.7
    pub mask_clearance_diameter_nm: Option<i64>,
    /// Bottom landing layer name (e.g., "metal3", "poly") — v0.2.2
    pub from_layer: Option<CompactString>,
    /// Top landing layer name (e.g., "metal4", "capm") — v0.2.2
    pub to_layer: Option<CompactString>,
}
