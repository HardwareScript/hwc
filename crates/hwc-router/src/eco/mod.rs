//! Freeze-Silicon Metal-Only ECO Routing Subsystem
//!
//! Implements Base Silicon (Layers 1-20) Immutability Verification, Craig
//! Interpolant Boolean Patch Mapping to GA-Fillers, and Metal 1-4 Jumper Routing.

pub mod metal_jumpers;
pub mod patch;

pub use metal_jumpers::MetalEcoRouter;
pub use patch::{EcoPatchManager, GaFillerCell};
