//! Real-Time Physics Validation using Bit-Parallel Sweeps (System 4)
//!
//! This module implements God-Tier physics validation using the BitChunk infrastructure
//! from System 2. Instead of checking voxels one-by-one, we use bitwise operations to
//! validate 64 voxels simultaneously.
//!
//! KEY INNOVATIONS:
//! - Parallel Page Sweeping: Use Rayon to distribute chunks across all CPU cores
//! - Dilation-Based Clearance: Bitwise dilation to check clearance in O(1) time
//! - Batch Validation: Check a million transistors for clearance in microseconds
//!
//! ARCHITECTURE:
//! ```text
//! VoxelGrid (4×4×4 chunks)
//!     ↓
//! Parallel Iterator (Rayon)
//!     ↓
//! BitChunk Validation (bitwise operations)
//!     ↓
//! Violation Reports
//! ```
//!
//! PERFORMANCE TARGET:
//! - 1 million voxels validated in < 10ms (100M voxels/sec)
//! - Multi-core scaling: 4 cores = 4× speedup
//! - Clearance checking: O(1) per chunk (not O(N) per voxel)

mod clearance;
mod dilation;
mod thermal;
mod types;
mod validator;
mod voltage;

// Re-export public types
pub use dilation::{dilate_mask_1d, dilate_mask_3d};
pub use types::{NetProperties, PhysicsValidationReport, PhysicsViolation};
pub use validator::PhysicsValidator;
