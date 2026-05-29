//! Process node definitions for different fabrication technologies

/// Fabrication process node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessNode {
    /// TSMC 5nm process
    TSMC5nm,
    /// TSMC 7nm process
    TSMC7nm,
    /// TSMC 14nm process
    TSMC14nm,
    /// Generic PCB process (for testing)
    GenericPCB,
}

impl ProcessNode {
    /// Get the minimum feature size in nanometers
    pub fn min_feature_size_nm(&self) -> i64 {
        match self {
            ProcessNode::TSMC5nm => 5,
            ProcessNode::TSMC7nm => 7,
            ProcessNode::TSMC14nm => 14,
            ProcessNode::GenericPCB => 100_000, // 0.1mm = 100µm
        }
    }

    /// Get the typical gate width in nanometers
    pub fn gate_width_nm(&self) -> i64 {
        match self {
            ProcessNode::TSMC5nm => 50,
            ProcessNode::TSMC7nm => 70,
            ProcessNode::TSMC14nm => 140,
            ProcessNode::GenericPCB => 1_000_000, // 1mm for discrete logic
        }
    }

    /// Get the typical gate height in nanometers
    pub fn gate_height_nm(&self) -> i64 {
        match self {
            ProcessNode::TSMC5nm => 100,
            ProcessNode::TSMC7nm => 140,
            ProcessNode::TSMC14nm => 280,
            ProcessNode::GenericPCB => 2_000_000, // 2mm for discrete logic
        }
    }

    /// Get the typical gate depth (Z-axis) in nanometers
    pub fn gate_depth_nm(&self) -> i64 {
        match self {
            ProcessNode::TSMC5nm => 20,
            ProcessNode::TSMC7nm => 30,
            ProcessNode::TSMC14nm => 50,
            ProcessNode::GenericPCB => 500_000, // 0.5mm for discrete logic
        }
    }
}
