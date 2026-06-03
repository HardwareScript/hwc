//! Hierarchical Bitmasked Chunked voxel grid - God-Tier spatial optimization.
//!
//! This module implements the ultimate voxel storage architecture using 4x4x4 bitmasked chunks
//! with a two-level page table system (no HashMaps in hot paths).
//!
//! TASK A2 COMPLETE: NetID Indirection
//! - VoxelChunks now store NetHandle instead of NetId
//! - NetLookupTable enables O(1) net renaming
//! - No voxel scanning required for renaming!

mod chunk;
mod conversion;
pub mod grid;
mod operations;
mod shared_buffer;
mod stats;
mod substrate_layers;

// Re-export public API
pub use chunk::{MaterialId, NetId};
pub use grid::{PlacementError, VoxelGrid};
pub use operations::CompactionStats;
pub use shared_buffer::{DirtyPageTracker, SharedBufferHeader, SharedVoxelBuffer, PAGE_SIZE};
pub use stats::MemoryStats;
pub use substrate_layers::{
    CapType, ComponentMetadata, ComponentPin, Cutout, LinerStack, Rotation, SubstrateLayer,
    SubstrateLayerShape, SubstrateLayerType, Terminal, TSVParams,
};
