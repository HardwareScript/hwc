// crates/hwc-synthesis/src/aig/mod.rs

pub mod arena;
pub mod fraig;

pub use arena::{Edge, PackedAigGraph, SequentialDff};
pub use fraig::FraigOptimizer;
