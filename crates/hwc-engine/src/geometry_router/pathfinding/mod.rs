//! A* Pathfinding Algorithm with Deterministic Tie-Breaking
//!
//! This module implements the core A* pathfinding algorithm with
//! deterministic behavior for reproducible builds.

mod collision;
mod cost;
mod heuristic;
mod router;
mod sdf_router;
mod state;
mod types;

// Re-export public API
pub use cost::calculate_move_cost;
pub use heuristic::heuristic;
pub use router::route_net_deterministic;
pub use sdf_router::route_net_sdf_accelerated;
pub use types::RoutingParams;
