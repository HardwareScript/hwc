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
pub mod copper_welder;
mod constraint_aware;
mod dummy_fill;
pub mod entity_graph;
pub mod geo_static_index;
mod htree;
mod layer_direction;
mod neighbor_generation;
mod parallel_router;
mod path_utils;
mod pathfinding;
mod polygon_rasterizer;
pub mod port_escape;
mod priority;
mod ripup;
mod route_lockfile;
mod router;
pub mod scene_graph;
pub mod stamp_parser;
mod routing_patterns;
mod sdf_generator;
pub mod spatial_index;
mod teardrops;
mod thermal_relief;
mod types;
pub mod route_decomposition;
pub mod negotiated_congestion;
pub mod partition;
pub mod soft_corridor;
pub mod topological_router;
pub mod multi_net_manager;
pub mod legalizer;
pub mod solvers;
pub mod compaction;
pub mod gcell_sweep;
pub mod incremental_drc;
pub mod connectivity_check;
pub mod parasitic_extraction;
pub mod em_thermal_check;
pub mod manufacturing_check;
pub mod stackup_slicing;
pub mod boundary_canonicalization;
pub mod geometry_refinement;
pub mod export_isolation;
pub mod query_engine;
pub mod incremental_dag;
pub mod deterministic_sort;
pub mod stable_hash_map;
pub mod lockfile;
pub mod miter_pass;
pub mod route_persistence;
pub mod i128_transforms;
pub mod deterministic_pathfinder;
pub mod deterministic_export;
pub mod integration_verification;

// Re-export public API
pub use bounding_box_tracker::{BoundingBoxTracker, TrackedObstacle};
pub use coarse_grid::{CoarseGrid, CoarseNode, COARSE_CELL_SIZE};
pub use collision_detection::check_clearance_violation;
pub use constraint_aware::{constraint_aware_astar, constraint_aware_heuristic, ConstraintNode};
pub use dummy_fill::{DummyFillConfig, DummyFillEngine, DummyFillStats};
pub use entity_graph::EntityGraph;
pub use geo_static_index::StaticLayerIndex;
pub use htree::{BufferScheduler, HTreeEngine, HTreeSegment};
pub use layer_direction::{assign_layer_directions, is_valid_move};
pub use neighbor_generation::{get_neighbors_stable, GridBounds};
pub use parallel_router::ParallelRouter;
pub use path_utils::calculate_path_length;
pub use pathfinding::{route_net_deterministic, RoutingParams};
pub use polygon_rasterizer::{Point2D, Polygon, PolygonRasterizer};
pub use port_escape::{
    calculate_circular_escape, calculate_rect_escape, parse_port_escape, CardinalPort, EdgeOffset,
    EscapePoint, NamedPosition,
};
pub use priority::NetPriority;
pub use ripup::{RipUpRouter, RipUpStats, RouteAttempt};
pub use route_lockfile::{
    compute_placement_hash, CompactLockfile, LockfileError, LockfileManager, RouteLockfile,
    LOCKFILE_VERSION,
};
pub use route_lockfile::{encode_arc, decode_arc, encode_instances, decode_instances};
pub use lockfile::{
    CompactLockfileBinary, ArchivedArcSegment, ArchivedComponentInstance,
    compute_fingerprint, compute_fingerprint_from_space,
    write_lockfile, load_lockfile, is_valid, LockfileData,
    traces_to_lockfile, lockfile_to_traces,
};
pub use router::GeometryRouter;
pub use routing_patterns::{
    LengthMatchingEngine, PatternStep, RoutedTrace, RoutingPattern, StandardPatterns,
};
pub use sdf_generator::SdfGenerator;
pub use spatial_index::{
    DynamicSpatialIndex, IndexedSegment, Point2Df64, query_overlapping_segments,
};
pub use teardrops::{IpcClass, TeardropConfig, TeardropEngine};
pub use thermal_relief::{
    RectangularPadParams, ThermalReliefConfig, ThermalReliefGenerator, ThermalReliefType,
};
pub use types::{NetRoute, RouteResult, RoutedNet, RoutingError, Via, ViaType};
pub use scene_graph::{ComponentInstance, ComponentStamp, OrientedBoundingBox, SceneGraph};
pub use stamp_parser::{
    bake_stamp, bake_stamp_from_rect, register_baked_stamps, stamp_pin_global_position,
};
pub use route_decomposition::{
    PinNode, RouteSegment, VirtualJunction, DecomposedNet,
    decompose_net, collect_pin_nodes, prim_mst, distance_matrix, detect_junctions,
};
pub use negotiated_congestion::{NegotiatedCongestionEngine, ResourceState};
pub use partition::{BoundaryPort, GCell, GCellId, PartitionGrid, partition_nets, shared_boundary_bounds};
pub use soft_corridor::{corridor_cost, generate_corridors, is_in_envelope, is_on_center_line, SoftCorridor};
pub use soft_corridor::cost as corridor_cost_levels;
pub use topological_router::{TopologicalRouter, TopologicalPath, SearchRay, RayDirection, RayIntersection};
pub use multi_net_manager::{MultiNetManager, NetRouteState, NetRoutingOrder, MultiNetStats};
pub use legalizer::{
    Legalizer, ClearanceViolation, LegalizationWindow, QpVariable, merge_windows, bbox_overlaps_2d,
};
pub use solvers::qp_solver::{QpSolver, QpSolution};
pub use solvers::dag_solver::{DagSolver, DagConstraint};
pub use compaction::{Compactor, SignalConstraints, CompactionMove};
