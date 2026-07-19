//! Main Geometry Router Implementation
//!
//! This module contains the GeometryRouter struct that orchestrates
//! the automatic routing process.

mod circular_operations;
pub mod core;
pub mod global_router;
mod routing_methods;
mod via_operations;

pub use core::GeometryRouter;
