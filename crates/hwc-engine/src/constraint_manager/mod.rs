//! Constraint Manager: Translates material properties into geometric constraints.
//!
//! This module implements Phase 1 of the 3-Phase Routing Sub-Pipeline.
//! It converts physics (voltage, current, material properties) into geometric
//! rules (clearance zones, trace widths) that guide the router.
//!
//! **Documentation References**:
//! - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 1-400, constraint translation)
//! - `Docs/v0.1.3/COMPILER-INTERNALS.md` (lines 400-600, Layer 3 Physical IR)

mod clearance;
mod crosstalk;
mod manager;
mod manager_impl;
mod trace_width;
mod types;

// Re-export public API
pub use clearance::calculate_clearance_nm;
pub use crosstalk::{calculate_crosstalk_penalty, calculate_parallel_length};
pub use manager::{ConstraintManager, SymbolTableTrait};
pub use manager_impl::constraint_generation::NetConstraintParams;
pub use manager_impl::domain::{Route, RoutedDomain, RoutingDomain};
pub use manager_impl::net_classification::{
    classify_nets, NetClassification, NetClassificationResult,
};
pub use trace_width::calculate_trace_width_nm;
pub use types::{
    ClearanceZone, ConstraintRulebook, FabricationConstraints, LayerDirection, RouteConstraints,
    StackupInfo,
};
