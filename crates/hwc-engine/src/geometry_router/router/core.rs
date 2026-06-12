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
/// Supports adaptive routing modes:
/// - **Pass-Through** (LOD 1): Small designs (<100 nets, <1mm²) bypass G-Cell
///   partitioning entirely. The router runs once over the whole board.
/// - **Hierarchical** (LOD 2): Large designs are partitioned into G-Cells,
///   routed globally, then detailed in parallel via Rayon.
///
/// **Documentation References**:
/// - `Docs/v0.1.3/ROUTING-AND-PHYSICS.md` (lines 400-800, routing pipeline)
/// - `Docs/v0.1.7/Adaptive-Heuristic.md` (adaptive mode selection)
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

    // =========================================================================
    // v0.1.7 Adaptive Router: Scale Detection & Mode Selection
    // =========================================================================

    /// Area threshold in nanometers² for switching to hierarchical mode.
    /// Designs with total area below this AND net count below
    /// `net_count_threshold` use Pass-Through mode (single G-Cell).
    /// Default: 1mm² = 1_000_000_000_000 nm²
    pub area_threshold_nm2: i64,

    /// Net count threshold for switching to hierarchical mode.
    /// Designs with net count below this AND area below
    /// `area_threshold_nm2` use Pass-Through mode.
    /// Default: 100
    pub net_count_threshold: usize,
}

/// Copper pour definition for anti-pad generation.
#[derive(Debug, Clone)]
pub struct CopperPour {
    pub(super) net_id: crate::netlist::NetId,
    pub(super) material_id: crate::voxel_grid::MaterialId,
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
            area_threshold_nm2: 1_000_000_000_000, // 1mm²
            net_count_threshold: 100,
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
        material_id: crate::voxel_grid::MaterialId,
        z_bottom_nm: i64,
        bounds: (Point3D, Point3D),
    ) {
        self.copper_pours.push(CopperPour {
            net_id,
            material_id,
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

    // =========================================================================
    // v0.1.7 Adaptive Router: Scale Detection & Mode Selection
    // =========================================================================

    /// Primary entrypoint for space-level routing with adaptive mode selection.
    ///
    /// Evaluates net count and total board area to choose between:
    /// - **Pass-Through Mode** (LOD 1): Small designs (<100 nets, <1mm²)
    ///   bypass G-Cell partitioning. The router runs once over the whole board.
    /// - **Hierarchical Mode** (LOD 2): Large designs are partitioned into
    ///   G-Cells via `CoarseGrid`, routed globally, then detailed in parallel
    ///   via Rayon.
    ///
    /// # Arguments
    /// * `grid_bbox` - Bounding box of the entire routing area
    /// * `nets` - Map of net IDs to their pin coordinates
    /// * `obstacle_bboxes` - Bounding boxes of all placed component obstacles
    /// * `substrate_layers` - Substrate layer data for reference-plane void detection
    /// * `net_frequencies` - Map of net IDs to their signal frequencies (Hz)
    ///
    /// # Returns
    /// Unified `RouteResult` containing all paths and vias, or a `RoutingError`.
    pub fn route_space(
        &mut self,
        grid_bbox: &crate::geometry::BoundingBox,
        nets: &FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>>,
        obstacle_bboxes: &[crate::geometry::BoundingBox],
        substrate_layers: Option<&[crate::voxel_grid::SubstrateLayer]>,
        net_frequencies: &FxHashMap<crate::netlist::NetId, f64>,
    ) -> Result<super::super::types::RouteResult, super::super::types::RoutingError> {
        let width = grid_bbox.max.x - grid_bbox.min.x;
        let height = grid_bbox.max.y - grid_bbox.min.y;
        let area_nm2 = width * height;
        let net_count = nets.len();

        if area_nm2 < self.area_threshold_nm2 && net_count < self.net_count_threshold {
            // --- PASS-THROUGH MODE ---
            eprintln!(
                "[ADAPTIVE ROUTER] Pass-Through mode: {} nets, area {:.2} mm² (below thresholds)",
                net_count,
                area_nm2 as f64 / 1_000_000_000_000.0
            );
            self.route_flat(nets, obstacle_bboxes, substrate_layers, net_frequencies)
        } else {
            // --- HIERARCHICAL MODE ---
            eprintln!(
                "[ADAPTIVE ROUTER] Hierarchical mode: {} nets, area {:.2} mm² (above thresholds)",
                net_count,
                area_nm2 as f64 / 1_000_000_000_000.0
            );
            self.route_hierarchical(grid_bbox, nets, obstacle_bboxes, substrate_layers, net_frequencies)
        }
    }

    /// Pass-Through routing: routes all nets in a single pass over the entire board.
    ///
    /// Used for small designs where G-Cell partitioning overhead would exceed
    /// the routing time itself. Multi-pin nets are routed using Steiner
    /// Minimum Tree approximation with dynamic target expansion (T-junctions).
    fn route_flat(
        &mut self,
        nets: &FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>>,
        _obstacle_bboxes: &[crate::geometry::BoundingBox],
        _substrate_layers: Option<&[crate::voxel_grid::SubstrateLayer]>,
        _net_frequencies: &FxHashMap<crate::netlist::NetId, f64>,
    ) -> Result<super::super::types::RouteResult, super::super::types::RoutingError> {
        // Use Steiner routing for all nets (dynamic target expansion for multi-pin nets)
        self.route_all_nets_steiner(nets)
    }

    /// Hierarchical routing: partition into G-Cells, global route, then parallel detailed routing.
    ///
    /// For large designs (SoC-scale), the board is divided into coarse G-Cells.
    /// Each G-Cell is assigned the nets that pass through it, then routed
    /// independently in parallel via Rayon. Results are stitched back into
    /// a unified `RouteResult`.
    ///
    /// **Architecture** (from `Docs/v0.1.7/Adaptive-Heuristic.md`):
    /// 1. Partition space into 3D G-Cell tiles
    /// 2. Assign nets to G-Cells based on pin locations
    /// 3. Route each G-Cell independently in parallel (Rayon)
    /// 4. Stitch localized routes back into a unified result
    fn route_hierarchical(
        &mut self,
        _grid_bbox: &crate::geometry::BoundingBox,
        nets: &FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>>,
        _obstacle_bboxes: &[crate::geometry::BoundingBox],
        _substrate_layers: Option<&[crate::voxel_grid::SubstrateLayer]>,
        _net_frequencies: &FxHashMap<crate::netlist::NetId, f64>,
    ) -> Result<super::super::types::RouteResult, super::super::types::RoutingError> {
        use rayon::prelude::*;

        // Step 1: Partition board into G-Cell regions
        let cell_size_nm = 10_000_000; // 10um coarse tiles
        let gcell_regions = self.partition_into_gcells(cell_size_nm);

        eprintln!(
            "[ADAPTIVE ROUTER] Partitioned into {} G-Cells ({}nm tiles)",
            gcell_regions.len(),
            cell_size_nm
        );

        // Step 2: Assign nets to G-Cells based on pin locations
        let gcell_nets = self.assign_nets_to_gcells(&gcell_regions, nets);

        // Step 3: Route each G-Cell in parallel via Rayon
        // Each G-Cell gets its own GeometryRouter clone with local bounds.
        let results: Vec<Result<super::super::types::RouteResult, super::super::types::RoutingError>> =
            gcell_regions
                .par_iter()
                .enumerate()
                .map(|(cell_idx, cell_bbox)| {
                    let cell_nets = gcell_nets.get(&cell_idx).cloned().unwrap_or_default();

                    if cell_nets.is_empty() {
                        return Ok(super::super::types::RouteResult::new());
                    }

                    // Create a dedicated router for this G-Cell
                    let mut cell_router = GeometryRouter {
                        bounds: GridBounds::new(
                            cell_bbox.max.x - cell_bbox.min.x,
                            cell_bbox.max.y - cell_bbox.min.y,
                            cell_bbox.max.z - cell_bbox.min.z,
                        ),
                        constraints: self.constraints.clone(),
                        layer_directions: self.layer_directions.clone(),
                        voxel_size_nm: self.voxel_size_nm,
                        occupied_voxels: FxHashMap::default(),
                        voxel_grid: crate::voxel_grid::VoxelGrid::new(
                            ((cell_bbox.max.x - cell_bbox.min.x) / self.voxel_size_nm).max(1) as usize,
                            ((cell_bbox.max.y - cell_bbox.min.y) / self.voxel_size_nm).max(1) as usize,
                            ((cell_bbox.max.z - cell_bbox.min.z) / self.voxel_size_nm).max(1).max(1) as usize,
                            crate::space::VoxelSize {
                                x_nm: self.voxel_size_nm,
                                y_nm: self.voxel_size_nm,
                                z_nm: self.voxel_size_nm,
                            },
                            0,
                        ),
                        vias: Vec::new(),
                        copper_pours: Vec::new(),
                        bounding_box_tracker: BoundingBoxTracker::new(),
                        area_threshold_nm2: self.area_threshold_nm2,
                        net_count_threshold: self.net_count_threshold,
                    };

                    // Route all nets assigned to this G-Cell using Steiner routing
                    let mut cell_result = super::super::types::RouteResult::new();

                    // Translate all pins to local G-Cell space for routing
                    let local_nets: FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>> =
                        cell_nets
                            .iter()
                            .map(|(&net_id, pins)| {
                                let local_pins: Vec<_> = pins
                                    .iter()
                                    .map(|pin| crate::geometry::Point3D::new(
                                        pin.x - cell_bbox.min.x,
                                        pin.y - cell_bbox.min.y,
                                        pin.z - cell_bbox.min.z,
                                    ))
                                    .collect();
                                (net_id, local_pins)
                            })
                            .collect();

                    // Use Steiner routing for multi-pin nets within this G-Cell
                    match cell_router.route_all_nets_steiner(&local_nets) {
                        Ok(local_result) => {
                            // Translate paths back to global coordinates
                            for (net_id, local_path) in &local_result.paths {
                                let global_path: Vec<_> = local_path
                                    .iter()
                                    .map(|pt| crate::geometry::Point3D::new(
                                        pt.x + cell_bbox.min.x,
                                        pt.y + cell_bbox.min.y,
                                        pt.z + cell_bbox.min.z,
                                    ))
                                    .collect();
                                cell_result.paths.insert(*net_id, global_path);
                            }
                            cell_result.vias.extend(local_result.vias);
                        }
                        Err(e) => {
                            eprintln!(
                                "[ADAPTIVE ROUTER] G-Cell {} Steiner routing failed: {:?}",
                                cell_idx, e
                            );
                            return Err(e);
                        }
                    }

                    Ok(cell_result)
                })
                .collect();

        // Step 4: Stitch all G-Cell results back into a unified result
        let mut final_result = super::super::types::RouteResult::new();
        for res in results {
            final_result.merge(res?);
        }

        eprintln!(
            "[ADAPTIVE ROUTER] Hierarchical routing complete: {} nets routed across {} G-Cells",
            final_result.paths.len(),
            gcell_regions.len()
        );

        Ok(final_result)
    }

    /// Partition the board into rectangular G-Cell regions.
    ///
    /// Each G-Cell is a `cell_size_nm × cell_size_nm` tile in XY,
    /// spanning the full Z depth.
    fn partition_into_gcells(
        &self,
        cell_size_nm: i64,
    ) -> Vec<crate::geometry::BoundingBox> {
        let width = self.bounds.width_nm;
        let height = self.bounds.height_nm;
        let depth = self.bounds.depth_nm;

        let cols = ((width + cell_size_nm - 1) / cell_size_nm).max(1);
        let rows = ((height + cell_size_nm - 1) / cell_size_nm).max(1);

        let mut regions = Vec::with_capacity((cols * rows) as usize);

        for row in 0..rows {
            for col in 0..cols {
                let x_min = col * cell_size_nm;
                let y_min = row * cell_size_nm;
                let x_max = ((col + 1) * cell_size_nm).min(width);
                let y_max = ((row + 1) * cell_size_nm).min(height);

                regions.push(crate::geometry::BoundingBox::new(
                    crate::geometry::Point3D::new(x_min, y_min, 0),
                    crate::geometry::Point3D::new(x_max, y_max, depth),
                ));
            }
        }

        regions
    }

    /// Assign nets to G-Cells based on pin locations.
    ///
    /// A net is assigned to a G-Cell if any of its pins fall within
    /// the cell's bounding box. A net may appear in multiple G-Cells
    /// if it spans cell boundaries.
    fn assign_nets_to_gcells(
        &self,
        gcell_regions: &[crate::geometry::BoundingBox],
        nets: &FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>>,
    ) -> FxHashMap<usize, FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>>> {
        let mut gcell_nets: FxHashMap<usize, FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>>> =
            FxHashMap::default();

        for (&net_id, pins) in nets {
            for (cell_idx, cell_bbox) in gcell_regions.iter().enumerate() {
                // Check if any pin of this net falls within this G-Cell
                let pins_in_cell: Vec<_> = pins
                    .iter()
                    .filter(|pin| cell_bbox.contains(**pin))
                    .copied()
                    .collect();

                if !pins_in_cell.is_empty() {
                    gcell_nets
                        .entry(cell_idx)
                        .or_default()
                        .insert(net_id, pins_in_cell);
                }
            }
        }

        gcell_nets
    }

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