pub mod constraint_manager;
pub mod design_rule_check;
pub mod geometry;
pub mod geometry_router;
pub mod layer_connection_database;
pub mod material;
pub mod netlist;
pub mod placement;
pub mod routing_database;
pub mod routing_layer_database;
pub mod space;
pub mod via_instance_database;
pub mod via_layer_mapping_database;

// Test utilities - available for doc tests and unit tests
pub mod test_utils;

// Re-export component_stamp public API (Sprint 2: Hierarchical Components)
pub use geometry_router::substrate_types::{ComponentMetadata, ComponentPin, Rotation, Terminal};
// Re-export constraint_manager public API (now modularized)
pub use constraint_manager::{
    calculate_clearance_nm, calculate_crosstalk_penalty, calculate_parallel_length,
    calculate_trace_width_nm, ClearanceZone, ConstraintManager, ConstraintRulebook, LayerDirection,
    RouteConstraints, SymbolTableTrait,
};
pub use design_rule_check::{
    report_to_errors, validate_clearances, validate_current_density, validate_physics_parallel,
    validate_trace_widths, violation_to_error, DesignRuleChecker, DrcError, DrcReport,
    DrcViolation,
};
pub use geometry::entity_ids::{
    ComponentGraphId, EntityId, GeometryGraphId, JunctionGraphId, NetGraphId, PinGraphId,
    RouteGraphId,
};
pub use geometry::transform::{BoundingBox2D, FixedTransform2D};
pub use geometry::{BoundingBox, Direction, Point3D, TraceSegment};
pub use geometry_router::em_thermal_check::{
    current_limit_ac_to_declaration, current_limit_dc, verify_em_thermal, AcCurrent,
    CurrentDeclaration, DrcViolation as EmThermalViolation, EmParams, ThermalParams,
};
pub use geometry_router::geometry_refinement::{
    canonicalize_contours, refine_geometry, refine_layer, RefinedContour,
};
pub use geometry_router::miter_pass::MiterEngine;
pub use geometry_router::scene_graph::{
    ComponentInstance, ComponentStamp, OrientedBoundingBox, SceneGraph,
};
pub use geometry_router::select_connection_candidates;
pub use geometry_router::spatial_index::{
    query_overlapping_segments, DynamicSpatialIndex, IndexedSegment,
};
pub use geometry_router::substrate_types::CompactionStats;
pub use geometry_router::{
    assign_layer_directions, is_valid_move, EntityGraph, GeometryRouter, GridBounds, NetRoute,
    RoutedNet, RoutingError,
};
pub use geometry_router::{
    bake_stamp, bake_stamp_from_rect, register_baked_stamps, stamp_pin_global_position,
};
pub use geometry_router::{
    corridor_cost, generate_corridors, partition_nets, BoundaryPort, GCell, GCellId, PartitionGrid,
    SoftCorridor,
};
pub use geometry_router::{decompose_net, DecomposedNet, PinNode, RouteSegment, VirtualJunction};
pub use geometry_router::{
    AccessRegion, ConnectionCandidate, CostComposer, CostEvaluator, DefaultRoutingDatabase,
    DerivedConstraint, IntentCostWeights, InterfaceCapability, InterfaceGeometry, InterfaceId,
    Normal2D, Orientation, PhysicalInterface, RoutingDatabase, RoutingIntent,
};
pub use geometry_router::{ClearanceViolation, LegalizationWindow, Legalizer};
pub use geometry_router::{CompactionMove, Compactor, SignalConstraints};
pub use geometry_router::{DagConstraint, DagSolver, QpSolution, QpSolver};
pub use geometry_router::{DummyFillConfig, DummyFillEngine, DummyFillStats};
pub use geometry_router::{MultiNetManager, MultiNetStats, TopologicalPath, TopologicalRouter};
pub use geometry_router::{PatternStep, RoutingPattern};
pub use geometry_router::{TeardropConfig, TeardropEngine};
pub use layer_connection_database::{
    ConnectionType, LayerConnectionDatabase, LayerConnectionError, RoutingConnectionPoint,
};
pub use material::{
    ManufacturingProcess, MaterialCategory, MaterialId, MaterialRegistry, AIR_MATERIAL_ID,
};
pub use netlist::{
    ArenaStats, ComponentData, ComponentId, NetData, NetId, NetlistArena, PinData, PinId,
};
pub use routing_database::{
    ConnectivityError, HierarchicalRoutingDatabase, ProvenanceSegment, RouteId, RouteSource,
    RoutingStatistics,
};
pub use routing_layer_database::{RoutingLayer, RoutingLayerDatabase, RoutingLayerError};
pub use space::{
    AnalyticTrace, ContactMetadata, Dimensions, HardwareSpace, KeepOutZone, LineSegment, PadShape,
    PourMetadata, SpaceView,
};
pub use via_instance_database::{ViaInstance, ViaInstanceDatabase};
pub use via_layer_mapping_database::{
    BridgeRuleInput, ViaConnection, ViaLayerMappingDatabase, ViaLayerMappingError,
};
