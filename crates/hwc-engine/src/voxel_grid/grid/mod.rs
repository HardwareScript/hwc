//! Modular VoxelGrid implementation

mod chunk_ops;
mod commit_ops;
mod core;
mod gpu_ops;
mod handle_ops;
mod stamp_ops; // Sprint 2: Component stamping
mod substrate_ops;
mod voxel_ops;

pub use core::VoxelGrid;
pub use stamp_ops::PlacementError;
