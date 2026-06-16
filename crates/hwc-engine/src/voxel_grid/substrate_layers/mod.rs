//! Substrate layer representation for O(1) memory sparse architecture.
//!
//! This module implements the God-Tier solution to substrate memory overhead.
//! Instead of storing substrates as millions of individual chunks, we store them
//! as bounding boxes with material IDs.
//!
//! MEMORY SAVINGS:
//! - Old: 2000x2000x2 substrate = 250,000 chunks = 84 MB
//! - New: 2000x2000x2 substrate = 1 layer = 32 bytes
//! - Improvement: 2,625,000x memory reduction!

mod component;
mod layer;
mod types;

pub use component::{ComponentMetadata, ComponentPin};
pub use layer::SubstrateLayer;
pub use types::{
    CapType, Cutout, LinerStack, Rotation, SubstrateLayerShape, SubstrateLayerType, TSVParams,
    Terminal,
};
