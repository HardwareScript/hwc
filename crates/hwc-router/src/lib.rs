//! HardwareScript v0.3.1: Tri-Hybrid Physical Router & Universal `wasm64` Extensibility Engine
//!
//! Subsystems:
//! - Stage 1 (PAA): Pin Access Analysis & Enclosure Scoring (`paa`)
//! - Stage 2 (Global): 14-byte SoA Volumetric Tensor & FastGR/Pathfinder (`global`)
//! - Stage 3 (Track Assignment): Panel Bipartite Matching & NPN Pin Swapping (`track_assign`)
//! - Stage 4 (Detailed): Dr. CU 2.0 Multi-Level Sparse-Grid A* & Timing RRR (`detailed`)
//! - Extensibility: Universal 64-bit Memory64 Pure Rust C-ABI (`ffi`)
//! - ECO: Freeze-Silicon Metal-Only Routing & GA-Filler Configuration (`eco`)

pub mod detailed;
pub mod eco;
pub mod engine;
pub mod ffi;
pub mod global;
pub mod paa;
pub mod track_assign;
pub mod traits;
pub mod types;

// Re-exports of primary types
pub use detailed::{DrcRules, SparseGridDetailedRouter, TimingSlackMap};
pub use eco::{EcoPatchManager, GaFillerCell, MetalEcoRouter};
pub use engine::TriHybridRouter;
pub use ffi::{
    HwcRoutingOutput64, HwcRoutingTask64, HwcViaInstance64, HwcWireSegment64, Wasm64RouterRunner,
};
pub use global::{CpuPathFinder, CudaFastGr, GlobalRouter};
pub use paa::{score_access_point, PaaScoringConfig, PinAccessAnalyzer};
pub use track_assign::{
    try_swap_symmetric_pins, BipartiteTrackAssigner, InputSymmetryGroup, TrackAssigner,
};
pub use traits::{RouterEngine, RoutingError, RoutingTask};
pub use types::{
    AccessPoint, AssignedTrackSegment, CutMaskPolygon, GCellVolume3D, PinAccessMap, RoutedOutput,
    RoutedTraceSegment, RoutedViaInstance, RoutingGuide, VolumetricTensor3D,
};
