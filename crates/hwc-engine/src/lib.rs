pub mod bit_chunk;
pub mod bulk_validator; // Task 4.3: Bulk Connection Validation
pub mod constraint_manager;
pub mod design_rule_check;
pub mod error_codes;
pub mod geometry;
pub mod geometry_router; // Now modularized into submodules
pub mod morton;
pub mod netlist;
pub mod physics_validator;
pub mod placement;
pub mod routing;
pub mod space;
pub mod voxel;
pub mod voxel_grid;
pub mod voxel_stamps;

// Test utilities - available for doc tests and unit tests
pub mod test_utils;

// Re-export bit_chunk public API
pub use bit_chunk::BitChunk;
// Re-export bulk_validator public API (Task 4.3: Bulk Connection Validation)
pub use bulk_validator::{BulkValidationError, BulkValidator};
// Re-export component_stamp public API (Sprint 2: Hierarchical Components)
pub use voxel_grid::{ComponentMetadata, Rotation, Terminal};
// Re-export constraint_manager public API (now modularized)
pub use constraint_manager::{
    calculate_clearance_nm, calculate_crosstalk_penalty, calculate_parallel_length,
    calculate_trace_width_nm, ClearanceZone, ConstraintManager, ConstraintRulebook, LayerDirection,
    RouteConstraints, SymbolTableTrait,
};
pub use design_rule_check::{
    report_to_errors, validate_clearances, validate_physics_parallel, validate_physics_sequential,
    validate_thermal, validate_trace_widths, violation_to_error, DesignRuleChecker, DrcError,
    DrcReport, DrcViolation, MaterialProperties, NetVoxels,
};
pub use geometry::{BoundingBox, Direction, Point3D, TraceSegment};
pub use geometry_router::{
    assign_layer_directions, calculate_move_cost, check_clearance_violation, get_neighbors_stable,
    heuristic, is_valid_move, is_voxel_available, mark_route_occupied, route_net_deterministic,
    route_net_sdf_accelerated, GeometryRouter, GridBounds, NetRoute, RoutedNet, RoutingError,
    SdfGenerator,
};
pub use morton::{morton_decode, morton_encode, morton_neighbor};
pub use netlist::{
    ArenaStats, ComponentData, ComponentId, NetData, NetId, NetlistArena, PinData, PinId,
};
pub use physics_validator::{PhysicsValidationReport, PhysicsValidator, PhysicsViolation};
pub use placement::{
    CollisionDetailedError, ComponentPlacer, PadShape, PlacementError, PlacementParams,
    SymbolTableTrait as PlacementSymbolTableTrait,
};
pub use routing::Router;
pub use space::{
    AnalyticTrace, ContactMetadata, Dimensions, GridCells, HardwareSpace, KeepOutZone, LineSegment,
    PourMetadata, SpaceView, VoxelSize,
};
pub use voxel::{
    MaterialConductivity, MaterialId, MaterialRegistry, ManufacturingProcess, AIR_MATERIAL_ID,
};
pub use voxel_grid::{CompactionStats, MemoryStats, NetId as VoxelNetId, VoxelGrid};
pub use voxel_stamps::{GateType, ProcessNode, VoxelLibrary, VoxelStamp};
