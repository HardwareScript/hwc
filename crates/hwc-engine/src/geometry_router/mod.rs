//! Geometry Router: Deterministic A* pathfinding with Manhattan routing.
//!
//! This module implements Phase 2 of the 3-Phase Routing Sub-Pipeline.
//! It performs deterministic pathfinding with physics constraints to route
//! nets automatically when users don't provide explicit waypoints.
//!
//! **GOD-TIER Architecture**: All voxel storage uses VoxelGrid with flat array indexing.
//! No HashMap-based voxel storage anywhere in the routing pipeline.
//!
//! **Documentation References**:
//! - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 400-800, Manhattan routing)
//! - `ROADMAP/v0.1.4/Gap3.md` (Hierarchical Parallel Routing)
//! - `ROADMAP/v0.1.5/ROUTING-GOD-TIER-MIGRATION.md` (VoxelGrid migration)

mod bounding_box_tracker;
mod coarse_grid;
mod collision_detection;
mod constraint_aware;
mod dummy_fill;
mod htree;
mod layer_direction;
mod teardrops;
mod neighbor_generation;
mod parallel_router;
mod path_utils;
mod pathfinding;
mod polygon_rasterizer;
mod priority;
mod ripup;
mod route_lockfile;
mod router;
mod routing_patterns;
mod sdf_generator;
mod thermal_relief;
mod types;

// Re-export public API
pub use bounding_box_tracker::{BoundingBoxTracker, TrackedObstacle};
pub use coarse_grid::{CoarseGrid, CoarseNode, COARSE_CELL_SIZE};
pub use collision_detection::{check_clearance_violation, is_voxel_available, mark_route_occupied};
pub use constraint_aware::{constraint_aware_astar, constraint_aware_heuristic, ConstraintNode};
pub use dummy_fill::{DummyFillConfig, DummyFillEngine, DummyFillStats};
pub use teardrops::{IpcClass, TeardropConfig, TeardropEngine};
pub use layer_direction::{assign_layer_directions, is_valid_move};
pub use neighbor_generation::{get_neighbors_stable, GridBounds};
pub use parallel_router::ParallelRouter;
pub use path_utils::calculate_path_length;
pub use pathfinding::{
    calculate_move_cost, heuristic, route_net_deterministic, route_net_sdf_accelerated,
    RoutingParams,
};
pub use polygon_rasterizer::{Point2D, Polygon, PolygonRasterizer};
pub use priority::NetPriority;
pub use ripup::{RipUpRouter, RipUpStats, RouteAttempt};
pub use route_lockfile::{GridMetadata, LockedRoute, LockfileManager, RouteLockfile};
pub use router::GeometryRouter;
pub use routing_patterns::{LengthMatchingEngine, PatternStep, RoutingPattern, RoutedTrace, StandardPatterns};
pub use sdf_generator::SdfGenerator;
pub use thermal_relief::{
    RectangularPadParams, ThermalReliefConfig, ThermalReliefGenerator, ThermalReliefType,
};
pub use types::{NetRoute, RoutedNet, RoutingError, Via, ViaType};
pub use htree::{BufferScheduler, HTreeEngine, HTreeSegment};
