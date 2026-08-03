//! Geometry Router: Topological ray-casting routing with Manhattan constraints.
//!
//! This module implements Phase 2 of the 3-Phase Routing Sub-Pipeline.
//! It performs deterministic pathfinding with physics constraints to route
//! nets automatically when users don't provide explicit waypoints.
//!
//! **Architecture**: The `TopologicalRouter` projects orthogonal search rays from
//! start/target ports and finds intersecting open-space paths. Collision detection
//! uses Minkowski-sum inflation against a per-layer `DynamicSpatialIndex`.
//! No grid-based storage — clearance is checked analytically during routing.
//!
//! **Documentation References**:
//! - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 400-800, Manhattan routing)
//! - `ROADMAP/v0.1.4/Gap3.md` (Hierarchical Parallel Routing)

mod bounding_box_tracker;
pub mod connection_candidate;
pub mod connection_interface;
pub mod constraints;
pub mod coordinate_system;
pub mod copper_welder;

pub mod boundary_canonicalization;
pub mod compaction;
pub mod connectivity_check;
pub mod deterministic_export;
pub mod deterministic_sort;
mod dummy_fill;
pub mod em_thermal_check;
pub mod entity_graph;
pub mod export_isolation;
pub mod gcell_sweep;
pub mod geometry_math;
pub mod geometry_refinement;
mod htree;
pub mod i128_transforms;
pub mod incremental_drc;
pub mod integration_verification;
pub mod interface_escape;
mod layer_direction;
pub mod legalizer;
pub mod lockfile;
pub mod manufacturing_check;
pub mod miter_pass;
pub mod multi_net_manager;
pub mod navigable_space;
mod neighbor_generation;
mod parallel_router;
pub mod parasitic_extraction;
pub mod partition;
mod path_utils;
pub mod port_escape;
pub mod priority;
pub mod query_engine;
pub mod route_decomposition;
pub mod route_persistence;
pub mod router;
pub mod routing_intent;
pub mod routing_patterns;
pub mod scene_graph;
pub mod soft_corridor;
pub mod solvers;
pub mod spatial_index;
pub mod stable_hash_map;
pub mod stackup_slicing;
mod stamp_parser;
pub mod static_geometry_guard;
pub mod substrate_types;
pub mod teardrops;
pub mod thermal_relief;
pub mod topological_router;
pub mod types;

mod geo_static_index;
pub use geo_static_index::StaticLayerIndex;
mod pathfinding;

// Re-export public API
pub use bounding_box_tracker::{BoundingBoxTracker, TrackedObstacle, ViaObstacleParams};
pub use compaction::{CompactionMove, Compactor, SignalConstraints};
pub use connection_candidate::{select_connection_candidates, ConnectionCandidate};
pub use connection_interface::{
    AccessRegion, DefaultRoutingDatabase, DerivedConstraint, InterfaceCapability,
    InterfaceGeometry, InterfaceId, Normal2D, Orientation, PhysicalInterface, RoutingDatabase,
};
pub use constraints::{
    check_constraints, HardConstraints, NetConstraints, RouteMetrics, SoftConstraints, Violation,
};
pub use dummy_fill::{DummyFillConfig, DummyFillEngine, DummyFillStats};
pub use em_thermal_check::{
    current_limit_ac_to_declaration, current_limit_dc, verify_em_thermal, AcCurrent,
    CurrentDeclaration, DrcViolation as EmThermalViolation, EmParams, ThermalParams,
};
pub use entity_graph::EntityGraph;
pub use pathfinding::{CostComposer, CostEvaluator};

pub use geometry_refinement::{
    canonicalize_contours, refine_geometry, refine_layer, RefinedContour,
};
pub use htree::{BufferScheduler, HTreeEngine, HTreeSegment};
pub use layer_direction::{assign_layer_directions, is_valid_move};
pub use legalizer::{
    bbox_overlaps_2d, merge_windows, ClearanceViolation, LegalizationWindow, Legalizer, QpVariable,
};
pub use lockfile::{
    build_layer_z_map, compute_fingerprint, compute_fingerprint_from_space, is_valid,
    load_lockfile, lockfile_to_traces, traces_to_lockfile, write_lockfile, ArchivedArcSegment,
    ArchivedComponentInstance, CompactLockfileBinary, LockfileData,
};
pub use multi_net_manager::{MultiNetManager, MultiNetStats, NetRouteState, NetRoutingOrder};
pub use navigable_space::{FreeCell, SemanticCost, SpatialDecomposer};
pub use neighbor_generation::GridBounds;
pub use parallel_router::ParallelRouter;
pub use partition::{
    partition_nets, shared_boundary_bounds, BoundaryPort, GCell, GCellId, PartitionGrid,
};
pub use path_utils::calculate_path_length;
pub use pathfinding::RoutingParams;
pub use port_escape::{
    calculate_circular_escape, calculate_rect_escape, parse_port_escape, CardinalPort, EdgeOffset,
    EscapePoint, NamedPosition,
};
pub use priority::{get_net_priority, NetPriorityMap};
pub use route_decomposition::{
    collect_pin_nodes, decompose_net, detect_junctions, distance_matrix, prim_mst, DecomposedNet,
    PinNode, RouteSegment, VirtualJunction,
};
pub use router::core::RouteSpaceRequest;
pub use router::GeometryRouter;
pub use routing_intent::{IntentCostWeights, RoutingIntent};
pub use routing_patterns::{
    LengthMatchingEngine, PatternStep, RoutedTrace, RoutingPattern, StandardPatterns,
};
pub use scene_graph::{ComponentInstance, ComponentStamp, OrientedBoundingBox, SceneGraph};
pub use soft_corridor::cost as corridor_cost_levels;
pub use soft_corridor::{
    corridor_cost, generate_corridors, is_in_envelope, is_on_center_line, SoftCorridor,
};
pub use solvers::dag_solver::{DagConstraint, DagSolver};
pub use solvers::qp_solver::{QpSolution, QpSolver};
pub use spatial_index::{query_overlapping_segments, DynamicSpatialIndex, IndexedSegment};
pub use stamp_parser::{
    bake_stamp, bake_stamp_from_rect, register_baked_stamps, stamp_pin_global_position,
};
pub use static_geometry_guard::{check_static_shorts, StaticViolation};
pub use substrate_types::CompactionStats;
pub use teardrops::{IpcClass, TeardropConfig, TeardropEngine, TeardropRequest};
pub use thermal_relief::{
    RectangularPadParams, ThermalReliefConfig, ThermalReliefGenerator, ThermalReliefType,
};
pub use topological_router::{
    RayDirection, RayIntersection, SearchRay, TopologicalPath, TopologicalRouter,
};
pub use types::{
    NetRoute, RouteResult, RoutedNet, RoutingError, RoutingHeuristics, Via, ViaSpec, ViaType,
};

// Re-export Technology from hwc-types
pub use hwc_types::Technology;
