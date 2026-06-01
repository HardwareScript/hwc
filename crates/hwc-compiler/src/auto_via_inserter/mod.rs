//! Automatic via insertion for layer transitions.
//!
//! This module implements Sprint 3.3: Automatic Via Insertion.
//! It detects when a net transitions between layers and automatically
//! inserts vias at overlap points to maintain electrical connectivity.
//!
//! # Architecture
//!
//! 1. Layer transition detection scans pours on a net for Z-layer changes.
//! 2. Overlap detection finds XY overlap regions between pours on different layers.
//! 3. Via stamping inserts vias at overlap centers or as arrays for power nets.

mod collision;
mod geometry;
mod inserter;
mod library;
mod placement;
#[cfg(test)]
mod tests;
mod types;

pub use inserter::AutoViaInserter;
pub use library::{ViaLibrary, ViaType};

pub(crate) use types::{LayerTransition, OverlapRegion, ViaArrayConfig};
