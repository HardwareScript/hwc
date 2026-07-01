//! Planar Island & Via Bridge (PIVB) Solver
//!
//! Topological connectivity verification that replaces coordinate-snapping with
//! a graph-based approach. Operates on three deterministic passes:
//!
//! 1. **Planar Island Extraction** — Nodes from pre-welded 2D contours
//! 2. **Vertical Bridge Mapping** — Edges from via/contact placements
//! 3. **Connectivity Validation** — Tarjan's SCC on the resulting graph
//!
//! Eliminates floating-point jitter and Z-depth sensitivity by recognizing
//! that electrical continuity is defined by two geometric primitives:
//! - Planar Islands (2D copper regions on a single layer)
//! - Vertical Bridges (vias/contacts that interconnect layers)
//!
//! ## Error Codes
//! - P41: Disconnected Net — Net has multiple disconnected components
//!
//! ## Integration
//! The PIVB solver consumes the same pre-welded 2D copper islands used by
//! the GLB/DXF/GDSII export stage, ensuring verification matches manufacturing.

mod graph;
mod solver;
mod types;

pub use graph::ConnectivityGraph;
pub use solver::{ContactPlacement, PivbSolver};
pub use types::{
    ConnectivityResult, FragmentationReport, FragmentedIsland, PlanarIsland, VerticalBridge,
};
