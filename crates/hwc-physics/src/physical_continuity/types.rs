use compact_str::CompactString;

use crate::connectivity::BoundingBox;

/// A conductive island - a group of physically-connected conductive geometry.
///
/// This represents all geometry that electrons can flow through without
/// crossing an insulator or vacuum. Each island should correspond to
/// exactly one logical net.
#[derive(Debug, Clone)]
pub struct ConductiveIsland {
    /// Unique island ID
    pub id: usize,

    /// All geometry nodes in this island
    pub nodes: Vec<GeometryNodeRef>,

    /// Bounding box of the entire island
    pub bbox: BoundingBox,

    /// Material ID (all nodes in an island must have the same material)
    pub material: u8,

    /// Pins that touch this island (populated during validation)
    pub pins: Vec<PinRef>,
}

/// Reference to a geometry node (pour, contact, or substrate layer).
///
/// This is a lightweight reference that can be used to look up the
/// actual geometry data in the original arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeometryNodeRef {
    /// Index into pours array
    Pour(usize),

    /// Index into contacts array
    Contact(usize),

    /// Index into substrate_layers array
    SubstrateLayer(usize),
}

/// Reference to a pin.
///
/// This represents a pin's position and identity for P43 detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PinRef {
    pub component_id: u32,
    pub pin_id: u32,
}

/// Pin position data for P43 detection.
///
/// This is a simple data structure that doesn't depend on the netlist arena,
/// avoiding circular dependencies between hwc-physics and hwc-engine.
#[derive(Debug, Clone)]
pub struct PinPosition {
    pub component_id: u32,
    pub pin_id: u32,
    pub x_nm: i64,
    pub y_nm: i64,
    pub z_nm: i64,
}

/// Binding between a logical net and physical islands.
///
/// This maps what the code says (net names) to what the physics says
/// (conductive islands). Mismatches indicate physical continuity violations.
#[derive(Debug, Clone)]
pub struct NetIslandBinding {
    /// Net name
    pub net_name: CompactString,

    /// Islands that claim to be part of this net
    pub islands: Vec<usize>, // Island IDs

    /// Pins that should be connected to this net (future use)
    pub expected_pins: Vec<PinRef>,
}

/// Physical continuity violation types.
///
/// These are the three critical errors that indicate the voxel model
/// doesn't match the logical netlist.
#[derive(Debug, Clone)]
pub enum PhysicalContinuityViolation {
    /// A net has multiple disconnected islands (P41: Physical Disconnection)
    ///
    /// This means the net name appears on multiple pieces of geometry
    /// that don't physically touch. Electrons cannot flow between them.
    DisconnectedNet {
        net_name: CompactString,
        island_count: usize,
        islands: Vec<IslandSummary>,
        suggested_fix: CompactString,
    },

    /// An island has multiple net labels (P42: Short Circuit)
    ///
    /// This means multiple net names appear on the same piece of
    /// physically-connected geometry. This is a short circuit.
    ShortCircuit {
        island_id: usize,
        net_names: Vec<CompactString>,
        overlap_location: CompactString,
        suggested_fix: CompactString,
    },

    /// An island has no pins (P43: Floating Conductor)
    ///
    /// This means there's conductive geometry that isn't connected to
    /// any component pin. It's electrically floating.
    FloatingConductor {
        island_id: usize,
        material_name: CompactString,
        bbox: BoundingBox,
        suggested_fix: CompactString,
    },
}

/// Summary of an island for error reporting.
///
/// This is a lightweight version of ConductiveIsland that's easier
/// to display in error messages.
#[derive(Debug, Clone)]
pub struct IslandSummary {
    pub id: usize,
    pub bbox: BoundingBox,
    pub pin_count: usize,
    pub node_count: usize,
}
