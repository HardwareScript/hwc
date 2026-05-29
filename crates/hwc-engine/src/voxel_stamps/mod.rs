//! Voxel Stamps - Pre-rasterized Standard Cell Library
//!
//! This module provides O(1) logic-to-physical conversion by storing pre-computed
//! voxel patterns for common logic gates. Instead of rasterizing rectangles for
//! each gate instance, we bitwise-OR pre-computed BitChunk patterns directly into
//! the VoxelGrid.
//!
//! # The Problem (Gap 4)
//! LogicSynthesizer creates AND gates, but the Rasterizer draws rectangles.
//! Without a high-speed library, converting logic to physical layout requires
//! O(N) rectangle rasterization per gate.
//!
//! # The God-Tier Solution
//! Pre-rasterize common gates into BitChunk arrays, then stamp them into the
//! VoxelGrid using bitwise-OR operations. This makes gate rasterization O(1).
//!
//! # Architecture
//! - `VoxelLibrary`: Stores pre-rasterized gate patterns
//! - `VoxelStamp`: A single pre-rasterized gate pattern
//! - `ProcessNode`: Technology node (TSMC-5nm, TSMC-7nm, etc.)
//! - `GateType`: AND, OR, NOT, NAND, NOR, XOR, MUX, etc.
//! - `TechMapper`: Maps profiles to VoxelLibraries (foundry-aware)
//! - `ProfileLibrary`: Maps profile names to process nodes

mod library;
mod process_node;
mod stamp;
mod tech_mapper;

pub use library::VoxelLibrary;
pub use process_node::ProcessNode;
pub use stamp::{GateType, VoxelStamp};
pub use tech_mapper::{ProfileLibrary, TechMapper};
