//! Voxel library - manages pre-rasterized gate patterns

use super::{GateType, ProcessNode, VoxelStamp};
use crate::voxel_grid::MaterialId;
use rustc_hash::FxHashMap;

/// Library of pre-rasterized voxel stamps for different process nodes
///
/// This is the God-Tier solution to Gap 4. Instead of rasterizing rectangles
/// for each gate, we store pre-computed voxel patterns and stamp them in O(1).
pub struct VoxelLibrary {
    /// Stamps organized by process node and gate type
    stamps: FxHashMap<(ProcessNode, GateType), VoxelStamp>,
}

impl VoxelLibrary {
    /// Create a new voxel library
    pub fn new() -> Self {
        let mut library = Self {
            stamps: FxHashMap::default(),
        };

        // Pre-populate with standard gates for common process nodes
        library.populate_generic_pcb();

        library
    }

    /// Get a stamp for a specific process node and gate type
    pub fn get_stamp(&self, process: ProcessNode, gate: GateType) -> Option<&VoxelStamp> {
        self.stamps.get(&(process, gate))
    }

    /// Add a custom stamp to the library
    pub fn add_stamp(&mut self, process: ProcessNode, stamp: VoxelStamp) {
        self.stamps.insert((process, stamp.gate_type), stamp);
    }

    /// Populate the library with GenericPCB stamps (for testing and discrete logic)
    fn populate_generic_pcb(&mut self) {
        let process = ProcessNode::GenericPCB;
        let copper: MaterialId = 2;

        // NOT gate (inverter) - simple 3-voxel pattern
        let not_voxels = vec![
            (0, 0, 0, copper), // Input
            (1, 0, 0, copper), // Body
            (2, 0, 0, copper), // Output
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::Not,
                not_voxels,
                (3, 1, 1),
                vec![(0, 0, 0)],
                vec![(2, 0, 0)],
            ),
        );

        // Buffer - same as NOT but semantically different
        let buffer_voxels = vec![(0, 0, 0, copper), (1, 0, 0, copper), (2, 0, 0, copper)];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::Buffer,
                buffer_voxels,
                (3, 1, 1),
                vec![(0, 0, 0)],
                vec![(2, 0, 0)],
            ),
        );

        // AND gate - 2 inputs, 1 output
        let and_voxels = vec![
            (0, 0, 0, copper), // Input A
            (0, 2, 0, copper), // Input B
            (1, 0, 0, copper), // Body
            (1, 1, 0, copper), // Body
            (1, 2, 0, copper), // Body
            (2, 1, 0, copper), // Output
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::And,
                and_voxels,
                (3, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(2, 1, 0)],
            ),
        );

        // OR gate - 2 inputs, 1 output
        let or_voxels = vec![
            (0, 0, 0, copper), // Input A
            (0, 2, 0, copper), // Input B
            (1, 0, 0, copper), // Body
            (1, 1, 0, copper), // Body
            (1, 2, 0, copper), // Body
            (2, 1, 0, copper), // Output
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::Or,
                or_voxels,
                (3, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(2, 1, 0)],
            ),
        );

        // NAND gate - 2 inputs, 1 output
        let nand_voxels = vec![
            (0, 0, 0, copper), // Input A
            (0, 2, 0, copper), // Input B
            (1, 0, 0, copper), // Body
            (1, 1, 0, copper), // Body
            (1, 2, 0, copper), // Body
            (2, 1, 0, copper), // Output
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::Nand,
                nand_voxels,
                (3, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(2, 1, 0)],
            ),
        );

        // NOR gate - 2 inputs, 1 output
        let nor_voxels = vec![
            (0, 0, 0, copper), // Input A
            (0, 2, 0, copper), // Input B
            (1, 0, 0, copper), // Body
            (1, 1, 0, copper), // Body
            (1, 2, 0, copper), // Body
            (2, 1, 0, copper), // Output
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::Nor,
                nor_voxels,
                (3, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(2, 1, 0)],
            ),
        );

        // XOR gate - 2 inputs, 1 output
        let xor_voxels = vec![
            (0, 0, 0, copper), // Input A
            (0, 2, 0, copper), // Input B
            (1, 0, 0, copper), // Body
            (1, 1, 0, copper), // Body
            (1, 2, 0, copper), // Body
            (2, 0, 0, copper), // Body
            (2, 1, 0, copper), // Body
            (2, 2, 0, copper), // Body
            (3, 1, 0, copper), // Output
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::Xor,
                xor_voxels,
                (4, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(3, 1, 0)],
            ),
        );

        // XNOR gate - 2 inputs, 1 output
        let xnor_voxels = vec![
            (0, 0, 0, copper), // Input A
            (0, 2, 0, copper), // Input B
            (1, 0, 0, copper), // Body
            (1, 1, 0, copper), // Body
            (1, 2, 0, copper), // Body
            (2, 0, 0, copper), // Body
            (2, 1, 0, copper), // Body
            (2, 2, 0, copper), // Body
            (3, 1, 0, copper), // Output
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::Xnor,
                xnor_voxels,
                (4, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(3, 1, 0)],
            ),
        );

        // MUX2 (2-to-1 multiplexer) - 3 inputs (data0, data1, select), 1 output
        let mux2_voxels = vec![
            (0, 0, 0, copper), // Input data0
            (0, 2, 0, copper), // Input data1
            (0, 4, 0, copper), // Input select
            (1, 0, 0, copper), // Body
            (1, 1, 0, copper), // Body
            (1, 2, 0, copper), // Body
            (1, 3, 0, copper), // Body
            (1, 4, 0, copper), // Body
            (2, 1, 0, copper), // Body
            (2, 2, 0, copper), // Body
            (2, 3, 0, copper), // Body
            (3, 2, 0, copper), // Output
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::Mux2,
                mux2_voxels,
                (4, 5, 1),
                vec![(0, 0, 0), (0, 2, 0), (0, 4, 0)],
                vec![(3, 2, 0)],
            ),
        );

        // D Flip-Flop - 3 inputs (D, clock, reset), 2 outputs (Q, Q_bar)
        let dff_voxels = vec![
            (0, 0, 0, copper), // Input D
            (0, 2, 0, copper), // Input clock
            (0, 4, 0, copper), // Input reset
            (1, 0, 0, copper), // Body
            (1, 1, 0, copper), // Body
            (1, 2, 0, copper), // Body
            (1, 3, 0, copper), // Body
            (1, 4, 0, copper), // Body
            (2, 0, 0, copper), // Body
            (2, 1, 0, copper), // Body
            (2, 2, 0, copper), // Body
            (2, 3, 0, copper), // Body
            (2, 4, 0, copper), // Body
            (3, 1, 0, copper), // Output Q
            (3, 3, 0, copper), // Output Q_bar
        ];
        self.add_stamp(
            process,
            VoxelStamp::new(
                GateType::DFlipFlop,
                dff_voxels,
                (4, 5, 1),
                vec![(0, 0, 0), (0, 2, 0), (0, 4, 0)],
                vec![(3, 1, 0), (3, 3, 0)],
            ),
        );
    }
}

impl Default for VoxelLibrary {
    fn default() -> Self {
        Self::new()
    }
}
