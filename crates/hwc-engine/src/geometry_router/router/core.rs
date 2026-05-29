//! Core GeometryRouter struct and initialization

use super::super::bounding_box_tracker::BoundingBoxTracker;
use super::super::neighbor_generation::GridBounds;
use crate::constraint_manager::{ConstraintRulebook, LayerDirection};
use crate::geometry::Point3D;
use rustc_hash::FxHashMap;

/// Geometry Router: Main routing engine.
///
/// Orchestrates the automatic routing process using A* pathfinding with
/// Manhattan routing constraints and physics-based clearance enforcement.
///
/// **Documentation Reference**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 400-800, routing pipeline)
pub struct GeometryRouter {
    /// Grid bounds for routing
    pub(super) bounds: GridBounds,

    /// Constraint rulebook (from Phase 1)
    pub(super) constraints: ConstraintRulebook,

    /// Layer directions for Manhattan routing
    pub(super) layer_directions: Vec<LayerDirection>,

    /// Voxel size in nanometers
    pub(super) voxel_size_nm: i64,

    /// Occupied voxels from previously routed nets
    /// Maps voxel position to the net that occupies it
    pub(super) occupied_voxels: FxHashMap<crate::geometry::Point3D, crate::netlist::NetId>,

    /// VoxelGrid for Binary Collision Skip optimization
    /// Enables O(1) chunk-based collision checking instead of O(N) hash lookups
    pub(super) voxel_grid: crate::voxel_grid::VoxelGrid,

    /// All vias placed during routing (for drill file generation)
    pub(super) vias: Vec<super::super::types::Via>,

    /// Copper pours (for anti-pad generation)
    pub(super) copper_pours: Vec<CopperPour>,

    /// v0.1.7 Minkowski Integration: BoundingBoxTracker for obstacle inflation.
    /// Stores all obstacles with pre-computed Minkowski-inflated AABBs.
    /// The SDF generator uses these inflated boxes to route zero-width rays,
    /// automatically satisfying all clearance constraints.
    pub(super) bounding_box_tracker: BoundingBoxTracker,
}

/// Copper pour definition for anti-pad generation.
#[derive(Debug, Clone)]
pub struct CopperPour {
    pub(super) net_id: crate::netlist::NetId,
    /// Bottom Z elevation of the pour plane in nanometers.
    pub(super) z_bottom_nm: i64,
    #[allow(dead_code)] // Reserved for future use in pour boundary checking
    pub(super) bounds: (Point3D, Point3D), // Min and max corners
}

impl GeometryRouter {
    /// Create a new geometry router.
    ///
    /// # Arguments
    /// * `bounds` - Grid bounds for routing
    /// * `constraints` - Constraint rulebook from constraint manager
    ///
    /// # Examples
    /// ```
    /// use hwc_engine::geometry_router::{GeometryRouter, GridBounds};
    /// use hwc_engine::constraint_manager::ConstraintRulebook;
    ///
    /// let bounds = GridBounds::new(50_000_000, 50_000_000, 10_000_000);
    /// let constraints = ConstraintRulebook::new(500_000);
    ///
    /// let router = GeometryRouter::new(bounds, constraints);
    /// ```
    pub fn new(bounds: GridBounds, constraints: ConstraintRulebook) -> Self {
        let voxel_size_nm = constraints.voxel_size_nm;

        // Extract layer directions from constraints
        let num_layers = constraints.layer_directions.len();
        let layer_directions = (0..num_layers)
            .map(|i| constraints.get_layer_direction(i))
            .collect();

        // Calculate VoxelGrid dimensions from bounds and voxel size
        let grid_x = ((bounds.width_nm / voxel_size_nm) as usize).max(1);
        let grid_y = ((bounds.height_nm / voxel_size_nm) as usize).max(1);
        let grid_z = ((bounds.depth_nm / voxel_size_nm) as usize).max(1);

        // Create VoxelSize for the grid
        let voxel_size = crate::space::VoxelSize {
            x_nm: voxel_size_nm,
            y_nm: voxel_size_nm,
            z_nm: voxel_size_nm,
        };

        // Create VoxelGrid for Binary Collision Skip
        let voxel_grid = crate::voxel_grid::VoxelGrid::new(grid_x, grid_y, grid_z, voxel_size, 0);

        Self {
            bounds,
            constraints,
            layer_directions,
            voxel_size_nm,
            occupied_voxels: FxHashMap::default(),
            voxel_grid,
            vias: Vec::new(),
            copper_pours: Vec::new(),
            bounding_box_tracker: BoundingBoxTracker::new(),
        }
    }

    /// Get all vias placed during routing (for drill file export).
    pub fn get_vias(&self) -> &[super::super::types::Via] {
        &self.vias
    }

    /// Clear a voxel (for rip-up and reroute).
    ///
    /// Removes the voxel from the occupied voxels map, making it available
    /// for routing again.
    pub fn clear_voxel(&mut self, point: Point3D) {
        self.occupied_voxels.remove(&point);
    }

    /// Add a copper pour to the router (for anti-pad generation).
    ///
    /// # Arguments
    /// * `net_id` - Net ID of the pour
    /// * `z_bottom_nm` - Bottom Z elevation of the pour plane
    /// * `bounds` - Bounding box of the pour
    pub fn add_copper_pour(
        &mut self,
        net_id: crate::netlist::NetId,
        z_bottom_nm: i64,
        bounds: (Point3D, Point3D),
    ) {
        self.copper_pours.push(CopperPour {
            net_id,
            z_bottom_nm,
            bounds,
        });
    }

    // =========================================================================
    // v0.1.7 Minkowski Obstacle Inflation Integration (Section 1.2)
    // =========================================================================

    /// Register a component obstacle with Minkowski inflation into the BoundingBoxTracker.
    ///
    /// This inflates the component's bounding box by `trace_width_nm / 2 + clearance_nm`
    /// in XY directions, so the SDF generator sees an already-inflated obstacle.
    /// The pathfinder routes a zero-width ray around these inflated boxes,
    /// automatically guaranteeing exact clearance with O(1) collision overhead.
    ///
    /// # Arguments
    /// * `bbox` - Component bounding box in nanometers
    /// * `trace_width_nm` - Width of traces being routed (nanometers)
    /// * `clearance_nm` - Minimum clearance to other nets (nanometers)
    /// * `name` - Component name (e.g., "R1", "Q1")
    /// * `component_type` - Component type (e.g., "Resistor")
    ///
    /// # Returns
    /// The inflation margin that was applied in nanometers.
    pub fn add_minkowski_obstacle(
        &mut self,
        bbox: crate::geometry::BoundingBox,
        trace_width_nm: i64,
        clearance_nm: i64,
        name: compact_str::CompactString,
        component_type: compact_str::CompactString,
    ) -> i64 {
        self.bounding_box_tracker.register_component(
            bbox,
            trace_width_nm,
            clearance_nm,
            name,
            component_type,
        )
    }

    /// Register a previously routed trace as an obstacle in the BoundingBoxTracker.
    ///
    /// This prevents the next trace from being routed too close to this one.
    /// The inflation ensures both trace width and inter-net clearance are maintained.
    ///
    /// # Arguments
    /// * `start` - Start point of the trace segment (nanometers)
    /// * `end` - End point of the trace segment (nanometers)
    /// * `existing_trace_width_nm` - Width of the existing trace
    /// * `new_trace_width_nm` - Width of the trace being routed now
    /// * `clearance_nm` - Minimum inter-trace clearance
    /// * `net_name` - Name of the net this trace belongs to
    ///
    /// # Returns
    /// The inflation margin that was applied in nanometers.
    pub fn add_minkowski_trace(
        &mut self,
        start: Point3D,
        end: Point3D,
        existing_trace_width_nm: i64,
        new_trace_width_nm: i64,
        clearance_nm: i64,
        net_name: compact_str::CompactString,
    ) -> i64 {
        self.bounding_box_tracker.register_trace(
            start,
            end,
            existing_trace_width_nm,
            new_trace_width_nm,
            clearance_nm,
            net_name,
        )
    }

    /// Get a reference to the BoundingBoxTracker for SDF generation.
    pub fn get_bounding_box_tracker(&self) -> &BoundingBoxTracker {
        &self.bounding_box_tracker
    }

    /// Clear all Minkowski-inflated obstacles for incremental compilation.
    pub fn clear_bounding_box_tracker(&mut self) {
        self.bounding_box_tracker.clear();
    }

    // =========================================================================
    // End Minkowski Integration
    // =========================================================================

    /// Add a component obstacle to the router's VoxelGrid (GAP3).
    ///
    /// This enables the router to avoid routing through components.
    /// Components are stored as sparse metadata (bounding boxes only).
    ///
    /// # Arguments
    /// * `bbox` - Component bounding box in nanometers
    /// * `material` - Material ID (e.g., 5 for Ceramic)
    /// * `name` - Component name (e.g., "R1", "Q1")
    /// * `component_type` - Component type (e.g., "Resistor")
    pub fn add_component_obstacle(
        &mut self,
        bbox: crate::geometry::BoundingBox,
        material: u8,
        name: compact_str::CompactString,
        component_type: compact_str::CompactString,
    ) {
        use smallvec::SmallVec;
        self.voxel_grid.add_component_metadata(
            bbox,
            material,
            name,
            component_type,
            SmallVec::new(),
        );
    }

    /// Add a component pin to the router's VoxelGrid (GAP3).
    ///
    /// This allows the router to route TO component pins (endpoints)
    /// while blocking routing THROUGH components.
    ///
    /// # Arguments
    /// * `x_nm` - X coordinate in nanometers (absolute)
    /// * `y_nm` - Y coordinate in nanometers (absolute)
    /// * `z_nm` - Z coordinate in nanometers (absolute)
    /// * `component_name` - Component instance name (e.g., "M1")
    /// * `pin_name` - Pin name within the component (e.g., "gate")
    /// * `net` - Optional net assignment (e.g., Some("VIN"))
    pub fn add_component_pin(
        &mut self,
        x_nm: i64,
        y_nm: i64,
        z_nm: i64,
        component_name: compact_str::CompactString,
        pin_name: compact_str::CompactString,
        net: Option<compact_str::CompactString>,
    ) {
        self.voxel_grid
            .add_component_pin(x_nm, y_nm, z_nm, component_name, pin_name, net);
    }
}