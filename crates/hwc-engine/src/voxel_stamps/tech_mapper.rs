//! Technology Mapping Pass - PDK-to-Stamp Binder
//!
//! This module provides foundry-aware technology mapping that binds logical gates
//! to physical VoxelStamps based on the target profile/process node.
//!
//! # The Problem
//! VoxelStamps exist, but LogicSynthesizer doesn't know which stamp to use for
//! which foundry. Same code should compile to different layouts for TSMC-5nm vs JLCPCB.
//!
//! # The Solution
//! ProfileLibrary maps profile names → ProcessNode → VoxelLibrary
//! This enables O(1) stamp lookup per gate with automatic fallback to generic stamps.
//!
//! # Architecture
//! ```text
//! Profile Name → ProcessNode → VoxelLibrary → VoxelStamp
//!     ↓              ↓              ↓              ↓
//! "ASIC_5nm"   TSMC5nm      Library     AND gate stamp
//! "PCB_Standard" GenericPCB  Library     AND gate stamp
//! ```

/// Type alias for generic stamp definition tuple
type GenericStampDef = (
    GateType,
    Vec<(usize, usize, usize, MaterialId)>,
    (usize, usize, usize),
    Vec<(usize, usize, usize)>,
    Vec<(usize, usize, usize)>,
);

use super::{GateType, ProcessNode, VoxelLibrary, VoxelStamp};
use crate::voxel_grid::MaterialId;
use compact_str::CompactString;
use rustc_hash::FxHashMap;

/// Maps profile names to process nodes and voxel libraries
///
/// This is the bridge between Hardware Script profiles and physical layouts.
/// It enables the same `.hw` code to compile to different physical layouts
/// based on the target process.
pub struct ProfileLibrary {
    /// Maps profile name → process node
    profile_to_process: FxHashMap<CompactString, ProcessNode>,

    /// Maps process node → voxel library
    process_to_library: FxHashMap<ProcessNode, VoxelLibrary>,

    /// Default process node for fallback
    default_process: ProcessNode,
}

impl ProfileLibrary {
    /// Create a new profile library with standard mappings
    pub fn new() -> Self {
        let mut library = Self {
            profile_to_process: FxHashMap::default(),
            process_to_library: FxHashMap::default(),
            default_process: ProcessNode::GenericPCB,
        };

        // Initialize standard profile mappings
        library.initialize_standard_profiles();

        // Initialize process libraries
        library.initialize_process_libraries();

        library
    }

    /// Initialize standard profile → process node mappings
    fn initialize_standard_profiles(&mut self) {
        // PCB profiles → GenericPCB
        self.profile_to_process
            .insert("PCB_Standard".into(), ProcessNode::GenericPCB);
        self.profile_to_process
            .insert("PCB_HighTemp".into(), ProcessNode::GenericPCB);
        self.profile_to_process
            .insert("PCB_HighVoltage".into(), ProcessNode::GenericPCB);

        // ASIC profiles → TSMC nodes
        self.profile_to_process
            .insert("ASIC_5nm".into(), ProcessNode::TSMC5nm);
        self.profile_to_process
            .insert("ASIC_7nm".into(), ProcessNode::TSMC7nm);
        self.profile_to_process
            .insert("ASIC_14nm".into(), ProcessNode::TSMC14nm);
        self.profile_to_process
            .insert("ASIC_180nm".into(), ProcessNode::TSMC14nm); // Use 14nm library for 180nm
    }

    /// Initialize voxel libraries for each process node
    fn initialize_process_libraries(&mut self) {
        // Get all generic stamp definitions
        let generic_stamps = Self::get_all_generic_stamps();

        // Create separate libraries for each process node
        // In a real implementation, each process would have unique stamps
        // For now, they all use the same stamp patterns

        for process in &[
            ProcessNode::GenericPCB,
            ProcessNode::TSMC5nm,
            ProcessNode::TSMC7nm,
            ProcessNode::TSMC14nm,
        ] {
            let mut library = VoxelLibrary::new();

            // Add stamps for this process node
            for (gate_type, voxels, dims, inputs, outputs) in &generic_stamps {
                library.add_stamp(
                    *process,
                    VoxelStamp::new(
                        *gate_type,
                        voxels.clone(),
                        *dims,
                        inputs.clone(),
                        outputs.clone(),
                    ),
                );
            }

            self.process_to_library.insert(*process, library);
        }
    }

    /// Get all generic stamp definitions
    /// Returns: Vec<(GateType, voxels, dimensions, inputs, outputs)>
    fn get_all_generic_stamps() -> Vec<GenericStampDef> {
        let copper: MaterialId = 2;

        vec![
            // NOT gate
            (
                GateType::Not,
                vec![(0, 0, 0, copper), (1, 0, 0, copper), (2, 0, 0, copper)],
                (3, 1, 1),
                vec![(0, 0, 0)],
                vec![(2, 0, 0)],
            ),
            // Buffer
            (
                GateType::Buffer,
                vec![(0, 0, 0, copper), (1, 0, 0, copper), (2, 0, 0, copper)],
                (3, 1, 1),
                vec![(0, 0, 0)],
                vec![(2, 0, 0)],
            ),
            // AND gate
            (
                GateType::And,
                vec![
                    (0, 0, 0, copper),
                    (0, 2, 0, copper),
                    (1, 0, 0, copper),
                    (1, 1, 0, copper),
                    (1, 2, 0, copper),
                    (2, 1, 0, copper),
                ],
                (3, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(2, 1, 0)],
            ),
            // OR gate
            (
                GateType::Or,
                vec![
                    (0, 0, 0, copper),
                    (0, 2, 0, copper),
                    (1, 0, 0, copper),
                    (1, 1, 0, copper),
                    (1, 2, 0, copper),
                    (2, 1, 0, copper),
                ],
                (3, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(2, 1, 0)],
            ),
            // NAND gate
            (
                GateType::Nand,
                vec![
                    (0, 0, 0, copper),
                    (0, 2, 0, copper),
                    (1, 0, 0, copper),
                    (1, 1, 0, copper),
                    (1, 2, 0, copper),
                    (2, 1, 0, copper),
                ],
                (3, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(2, 1, 0)],
            ),
            // NOR gate
            (
                GateType::Nor,
                vec![
                    (0, 0, 0, copper),
                    (0, 2, 0, copper),
                    (1, 0, 0, copper),
                    (1, 1, 0, copper),
                    (1, 2, 0, copper),
                    (2, 1, 0, copper),
                ],
                (3, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(2, 1, 0)],
            ),
            // XOR gate
            (
                GateType::Xor,
                vec![
                    (0, 0, 0, copper),
                    (0, 2, 0, copper),
                    (1, 0, 0, copper),
                    (1, 1, 0, copper),
                    (1, 2, 0, copper),
                    (2, 0, 0, copper),
                    (2, 1, 0, copper),
                    (2, 2, 0, copper),
                    (3, 1, 0, copper),
                ],
                (4, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(3, 1, 0)],
            ),
            // XNOR gate
            (
                GateType::Xnor,
                vec![
                    (0, 0, 0, copper),
                    (0, 2, 0, copper),
                    (1, 0, 0, copper),
                    (1, 1, 0, copper),
                    (1, 2, 0, copper),
                    (2, 0, 0, copper),
                    (2, 1, 0, copper),
                    (2, 2, 0, copper),
                    (3, 1, 0, copper),
                ],
                (4, 3, 1),
                vec![(0, 0, 0), (0, 2, 0)],
                vec![(3, 1, 0)],
            ),
            // MUX2
            (
                GateType::Mux2,
                vec![
                    (0, 0, 0, copper),
                    (0, 2, 0, copper),
                    (0, 4, 0, copper),
                    (1, 0, 0, copper),
                    (1, 1, 0, copper),
                    (1, 2, 0, copper),
                    (1, 3, 0, copper),
                    (1, 4, 0, copper),
                    (2, 1, 0, copper),
                    (2, 2, 0, copper),
                    (2, 3, 0, copper),
                    (3, 2, 0, copper),
                ],
                (4, 5, 1),
                vec![(0, 0, 0), (0, 2, 0), (0, 4, 0)],
                vec![(3, 2, 0)],
            ),
            // D Flip-Flop
            (
                GateType::DFlipFlop,
                vec![
                    (0, 0, 0, copper),
                    (0, 2, 0, copper),
                    (0, 4, 0, copper),
                    (1, 0, 0, copper),
                    (1, 1, 0, copper),
                    (1, 2, 0, copper),
                    (1, 3, 0, copper),
                    (1, 4, 0, copper),
                    (2, 0, 0, copper),
                    (2, 1, 0, copper),
                    (2, 2, 0, copper),
                    (2, 3, 0, copper),
                    (2, 4, 0, copper),
                    (3, 1, 0, copper),
                    (3, 3, 0, copper),
                ],
                (4, 5, 1),
                vec![(0, 0, 0), (0, 2, 0), (0, 4, 0)],
                vec![(3, 1, 0), (3, 3, 0)],
            ),
        ]
    }

    /// Register a custom profile → process node mapping
    ///
    /// This allows users to define custom profiles and map them to process nodes.
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::voxel_stamps::{ProfileLibrary, ProcessNode};
    /// let mut library = ProfileLibrary::new();
    /// library.register_profile("MyCustomProfile", ProcessNode::TSMC7nm);
    /// ```
    pub fn register_profile(&mut self, profile_name: impl Into<String>, process: ProcessNode) {
        self.profile_to_process
            .insert(profile_name.into().into(), process);
    }

    /// Get the process node for a profile name
    ///
    /// Returns the default process node if the profile is not found.
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::voxel_stamps::{ProfileLibrary, ProcessNode};
    /// let library = ProfileLibrary::new();
    /// let process = library.get_process_node("ASIC_5nm");
    /// assert_eq!(process, ProcessNode::TSMC5nm);
    /// ```
    pub fn get_process_node(&self, profile_name: &str) -> ProcessNode {
        self.profile_to_process
            .get(profile_name)
            .copied()
            .unwrap_or(self.default_process)
    }

    /// Get a voxel stamp for a specific profile and gate type
    ///
    /// This is the main API for technology mapping. It performs O(1) lookup:
    /// 1. Profile name → Process node
    /// 2. Process node → VoxelLibrary
    /// 3. Gate type → VoxelStamp
    ///
    /// Falls back to default process if profile not found.
    /// Returns None if gate type not available in library.
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::voxel_stamps::{ProfileLibrary, GateType};
    /// let library = ProfileLibrary::new();
    /// let stamp = library.get_stamp("ASIC_5nm", GateType::And);
    /// assert!(stamp.is_some());
    /// ```
    pub fn get_stamp(&self, profile_name: &str, gate_type: GateType) -> Option<&VoxelStamp> {
        let process = self.get_process_node(profile_name);
        let library = self.process_to_library.get(&process)?;
        library.get_stamp(process, gate_type)
    }

    /// Get the voxel library for a specific profile
    ///
    /// Returns None if the profile's process node doesn't have a library.
    pub fn get_library(&self, profile_name: &str) -> Option<&VoxelLibrary> {
        let process = self.get_process_node(profile_name);
        self.process_to_library.get(&process)
    }

    /// Get the voxel library for a specific process node
    pub fn get_library_for_process(&self, process: ProcessNode) -> Option<&VoxelLibrary> {
        self.process_to_library.get(&process)
    }

    /// Set the default process node for fallback
    pub fn set_default_process(&mut self, process: ProcessNode) {
        self.default_process = process;
    }

    /// Get the default process node
    pub fn default_process(&self) -> ProcessNode {
        self.default_process
    }

    /// Check if a profile is registered
    pub fn has_profile(&self, profile_name: &str) -> bool {
        self.profile_to_process.contains_key(profile_name)
    }

    /// Get all registered profile names
    pub fn profile_names(&self) -> Vec<&str> {
        self.profile_to_process.keys().map(|s| s.as_str()).collect()
    }

    /// Get all supported process nodes
    pub fn process_nodes(&self) -> Vec<ProcessNode> {
        self.process_to_library.keys().copied().collect()
    }
}

impl Default for ProfileLibrary {
    fn default() -> Self {
        Self::new()
    }
}

/// Technology mapper for logic-to-physical conversion
///
/// This is the high-level API for mapping logical gates to physical stamps
/// during the compilation process.
pub struct TechMapper {
    library: ProfileLibrary,
}

impl TechMapper {
    /// Create a new technology mapper
    pub fn new() -> Self {
        Self {
            library: ProfileLibrary::new(),
        }
    }

    /// Create a technology mapper with a custom profile library
    pub fn with_library(library: ProfileLibrary) -> Self {
        Self { library }
    }

    /// Map a logical gate to a physical stamp
    ///
    /// This is the main entry point for technology mapping during compilation.
    /// It performs O(1) lookup from profile + gate type → VoxelStamp.
    ///
    /// # Arguments
    /// * `profile_name` - Target profile (e.g., "ASIC_5nm", "PCB_Standard")
    /// * `gate_type` - Logical gate type (AND, OR, NOT, etc.)
    ///
    /// # Returns
    /// * `Some(&VoxelStamp)` - Physical stamp for the gate
    /// * `None` - Gate type not available in library
    ///
    /// # Example
    /// ```
    /// # use hwc_engine::voxel_stamps::{TechMapper, GateType};
    /// let mapper = TechMapper::new();
    /// let stamp = mapper.map_logic_to_stamp("PCB_Standard", GateType::And);
    /// assert!(stamp.is_some());
    /// ```
    pub fn map_logic_to_stamp(
        &self,
        profile_name: &str,
        gate_type: GateType,
    ) -> Option<&VoxelStamp> {
        self.library.get_stamp(profile_name, gate_type)
    }

    /// Get the profile library
    pub fn library(&self) -> &ProfileLibrary {
        &self.library
    }

    /// Get mutable access to the profile library
    pub fn library_mut(&mut self) -> &mut ProfileLibrary {
        &mut self.library
    }

    /// Map multiple gates at once (batch operation)
    ///
    /// This is useful for mapping an entire netlist in one pass.
    ///
    /// # Returns
    /// Vector of (gate_type, Option<&VoxelStamp>) tuples
    pub fn map_logic_batch(
        &self,
        profile_name: &str,
        gate_types: &[GateType],
    ) -> Vec<(GateType, Option<&VoxelStamp>)> {
        gate_types
            .iter()
            .map(|&gate_type| (gate_type, self.map_logic_to_stamp(profile_name, gate_type)))
            .collect()
    }

    /// Check if all gates in a list are available for a profile
    ///
    /// Returns true only if ALL gates have stamps in the library.
    pub fn all_gates_available(&self, profile_name: &str, gate_types: &[GateType]) -> bool {
        gate_types
            .iter()
            .all(|&gate_type| self.map_logic_to_stamp(profile_name, gate_type).is_some())
    }

    /// Get missing gates for a profile
    ///
    /// Returns a list of gate types that don't have stamps in the library.
    pub fn missing_gates(&self, profile_name: &str, gate_types: &[GateType]) -> Vec<GateType> {
        gate_types
            .iter()
            .filter(|&&gate_type| self.map_logic_to_stamp(profile_name, gate_type).is_none())
            .copied()
            .collect()
    }
}

impl Default for TechMapper {
    fn default() -> Self {
        Self::new()
    }
}
