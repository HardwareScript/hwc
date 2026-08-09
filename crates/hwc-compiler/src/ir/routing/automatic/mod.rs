//! Automatic routing using topological ray-casting.
//!
//! This module implements the 3-phase automatic routing pipeline:
//! 1. Constraint Manager: Generate geometric constraints from physics
//! 2. Geometry Router: Topological ray-casting with Manhattan routing
//! 3. Design Rule Check: Validate physics compliance

mod boundary;
mod constraints;
mod geometry;
mod pipeline;
mod verification;

pub use boundary::{
    calculate_boundary_points, select_routable_port_from_resolution, PortSelectionParams,
};
pub use pipeline::route_automatic;
