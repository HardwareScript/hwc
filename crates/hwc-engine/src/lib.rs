pub mod constraint_manager;
pub mod design_rule_check;
pub mod geometry;
pub mod geometry_router;
pub mod material;
pub mod netlist;
pub mod space;
pub mod placement;

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
    report_to_errors, validate_clearances, validate_physics_parallel, validate_physics_sequential,
    validate_thermal_analytic, validate_trace_widths, violation_to_error, DesignRuleChecker, DrcError,
    DrcReport, DrcViolation,
};
pub use geometry::{BoundingBox, Direction, Point3D, TraceSegment};
pub use geometry::transform::{BoundingBox2D, FixedTransform2D};
pub use geometry::entity_ids::{
    EntityId, ComponentGraphId, PinGraphId, NetGraphId,
    RouteGraphId, GeometryGraphId, JunctionGraphId,
};
pub use geometry_router::{
    assign_layer_directions, check_clearance_violation, get_neighbors_stable,
    is_valid_move, route_net_sdf_accelerated,
    EntityGraph, GeometryRouter, GridBounds, NetRoute, RoutedNet,
    RoutingError, SdfGenerator,
};
pub use geometry_router::{RoutingPattern, PatternStep};
pub use geometry_router::miter_pass::MiterEngine;
pub use geometry_router::{
    BoundaryPort, GCell, GCellId, PartitionGrid, SoftCorridor, corridor_cost,
    generate_corridors, partition_nets,
};
pub use geometry_router::spatial_index::{DynamicSpatialIndex, IndexedSegment, query_overlapping_segments};
pub use geometry_router::{
    TopologicalRouter, TopologicalPath, MultiNetManager, MultiNetStats,
};
pub use geometry_router::{Legalizer, ClearanceViolation, LegalizationWindow};
pub use geometry_router::{QpSolver, QpSolution, DagSolver, DagConstraint};
pub use geometry_router::{Compactor, SignalConstraints, CompactionMove};
pub use geometry_router::{DummyFillConfig, DummyFillEngine, DummyFillStats};
pub use geometry_router::{TeardropConfig, TeardropEngine};
pub use geometry_router::geo_static_index::StaticLayerIndex;
pub use geometry_router::em_thermal_check::{
    AcCurrent, CurrentDeclaration, EmParams, ThermalParams,
    current_limit_ac_to_declaration, current_limit_dc, verify_em_thermal,
    DrcViolation as EmThermalViolation,
};
pub use geometry_router::scene_graph::{ComponentStamp, ComponentInstance, OrientedBoundingBox, SceneGraph};
pub use geometry_router::{
    bake_stamp, bake_stamp_from_rect, register_baked_stamps, stamp_pin_global_position,
};
pub use geometry_router::{
    PinNode, RouteSegment, VirtualJunction, DecomposedNet, decompose_net,
    NegotiatedCongestionEngine,
};
pub use geometry_router::geometry_refinement::{RefinedContour, refine_layer, refine_geometry, canonicalize_contours};
pub use geometry_router::substrate_types::{CompactionStats, MemoryStats};
pub use netlist::{
    ArenaStats, ComponentData, ComponentId, NetData, NetId, NetlistArena, PinData, PinId,
};
pub use space::{
    AnalyticTrace, ContactMetadata, Dimensions, HardwareSpace, KeepOutZone, LineSegment,
    PourMetadata, SpaceView, PadShape,
};
pub use material::{
    ManufacturingProcess, MaterialConductivity, MaterialId, MaterialRegistry, AIR_MATERIAL_ID,
};
