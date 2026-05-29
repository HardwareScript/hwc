//! Voxel stamp definitions - pre-rasterized gate patterns

use crate::voxel_grid::{MaterialId, NetId, VoxelGrid};

/// Logic gate types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GateType {
    /// AND gate (2-input)
    And,
    /// OR gate (2-input)
    Or,
    /// NOT gate (inverter)
    Not,
    /// NAND gate (2-input)
    Nand,
    /// NOR gate (2-input)
    Nor,
    /// XOR gate (2-input)
    Xor,
    /// XNOR gate (2-input)
    Xnor,
    /// 2-to-1 Multiplexer
    Mux2,
    /// Buffer
    Buffer,
    /// D Flip-Flop
    DFlipFlop,
}

impl GateType {
    /// Get the number of input pins for this gate type
    pub fn input_count(&self) -> usize {
        match self {
            GateType::Not | GateType::Buffer => 1,
            GateType::And
            | GateType::Or
            | GateType::Nand
            | GateType::Nor
            | GateType::Xor
            | GateType::Xnor => 2,
            GateType::Mux2 => 3,      // data0, data1, select
            GateType::DFlipFlop => 3, // D, clock, reset
        }
    }

    /// Get the number of output pins for this gate type
    pub fn output_count(&self) -> usize {
        match self {
            GateType::DFlipFlop => 2, // Q, Q_bar
            _ => 1,
        }
    }
}

/// A pre-rasterized voxel pattern for a logic gate
///
/// This stores the exact voxel coordinates and materials for a gate,
/// allowing O(1) stamping into the VoxelGrid.
#[derive(Debug, Clone)]
pub struct VoxelStamp {
    /// Gate type
    pub gate_type: GateType,

    /// Pre-computed voxel positions (relative coordinates)
    /// Format: (x, y, z, material_id)
    pub voxels: Vec<(usize, usize, usize, MaterialId)>,

    /// Bounding box dimensions (width, height, depth) in voxels
    pub dimensions: (usize, usize, usize),

    /// Input pin positions (relative coordinates)
    pub input_pins: Vec<(usize, usize, usize)>,

    /// Output pin positions (relative coordinates)
    pub output_pins: Vec<(usize, usize, usize)>,
}

impl VoxelStamp {
    /// Create a new voxel stamp
    pub fn new(
        gate_type: GateType,
        voxels: Vec<(usize, usize, usize, MaterialId)>,
        dimensions: (usize, usize, usize),
        input_pins: Vec<(usize, usize, usize)>,
        output_pins: Vec<(usize, usize, usize)>,
    ) -> Self {
        Self {
            gate_type,
            voxels,
            dimensions,
            input_pins,
            output_pins,
        }
    }

    /// Stamp this pattern into the VoxelGrid at the specified position
    ///
    /// This is the God-Tier O(1) operation that makes logic-to-physical conversion fast.
    ///
    /// # Arguments
    /// * `grid` - Target VoxelGrid
    /// * `origin` - Origin position (x, y, z) in voxels
    /// * `net` - Net ID to assign to all voxels
    ///
    /// # Performance
    /// O(V) where V is the number of voxels in the stamp (typically 10-100)
    /// This is O(1) relative to the grid size and much faster than rectangle rasterization
    pub fn stamp_into_grid(&self, grid: &mut VoxelGrid, origin: (usize, usize, usize), net: NetId) {
        let (ox, oy, oz) = origin;

        for &(x, y, z, material) in &self.voxels {
            let abs_x = ox + x;
            let abs_y = oy + y;
            let abs_z = oz + z;

            grid.set_occupied(
                abs_x,
                abs_y,
                abs_z,
                material,
                crate::netlist::NetHandle::new(net),
            );
        }
    }

    /// Get the absolute position of an input pin
    pub fn get_input_pin_position(
        &self,
        origin: (usize, usize, usize),
        pin_index: usize,
    ) -> Option<(usize, usize, usize)> {
        self.input_pins
            .get(pin_index)
            .map(|&(x, y, z)| (origin.0 + x, origin.1 + y, origin.2 + z))
    }

    /// Get the absolute position of an output pin
    pub fn get_output_pin_position(
        &self,
        origin: (usize, usize, usize),
        pin_index: usize,
    ) -> Option<(usize, usize, usize)> {
        self.output_pins
            .get(pin_index)
            .map(|&(x, y, z)| (origin.0 + x, origin.1 + y, origin.2 + z))
    }
}
