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

    /// v0.1.8 continuous database snap-step resolution in nanometers
    pub(super) resolution_nm: i64,

    /// EntityGraph for component metadata, substrate layers, and spatial queries.
    /// Replaces the legacy VoxelGrid for all engine-internal operations.
    pub(super) entity_graph: super::super::EntityGraph,

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

    // =========================================================================
    // v0.1.7 ASIC Via Tower Unrolling
    // =========================================================================
    /// True for ASIC (Manhattan angle restriction), false for PCB (Octilinear).
    /// When true, multi-layer via transitions are unrolled into layer-by-layer
    /// buried vias with intermediate landing pads.
    pub(super) is_manhattan: bool,

    /// Ordered layer names from the stackup profile (bottom-to-top).
    /// Used by `unroll_via_tower` to step through layers one at a time.
    pub(super) profile_layers: Vec<String>,

    /// Z start position (bottom) in nanometers for each layer in `profile_layers`.
    /// Parallel array: `layer_z_positions[i]` corresponds to `profile_layers[i]`.
    pub(super) layer_z_positions: Vec<i64>,

    // =========================================================================
    // v0.1.7 Substrate & Reference-Plane Aware Routing
    // =========================================================================
    /// Substrate layers for reference-plane void detection.
    /// Set via `set_substrate_context()` before routing.
    pub(super) substrate_layers: Option<Vec<crate::geometry_router::substrate_types::SubstrateLayer>>,

    /// Net frequencies in Hz (e.g., 5_000_000_000.0 for 5 GHz).
    /// Set via `set_substrate_context()` before routing.
    pub(super) net_frequencies: FxHashMap<crate::netlist::NetId, f64>,

    /// Coarse partition grid for hierarchical G-Cell routing.
    /// Created in `route_space()` before routing branches to enable
    /// topological routing with partition-guided pathfinding.
    pub partition_grid: Option<super::super::partition::PartitionGrid>,

    /// v0.1.8 Salsa-style memoized query store for per-G-cell routing cache.
    /// When present, hierarchical G-Cell routing results are memoized so that
    /// unchanged G-cells return cached results on incremental rebuilds.
    pub query_store: Option<super::super::query_engine::QueryStore>,

    /// v0.1.8: Per-net routing pattern policies from `route net:` statements.
    /// Maps NetId -> RoutingPattern. When a net has a policy, the Steiner
    /// decomposition uses `route_net_with_length_constraint` instead of
    /// `route_net_global` to inject pattern macro-moves during routing.
    pub route_net_policies: FxHashMap<crate::netlist::NetId, super::super::routing_patterns::RoutingPattern>,

    /// v0.1.8: SDF (Signed Distance Field) generator for Leap-Frog A* routing.
    /// Built in `route_space()` from the EntityGraph after component registration.
    /// When present, `route_net_global()` uses SDF-accelerated A* instead of
    /// the legacy TopologicalRouter, enabling guardrails (R25, Interior Lockout,
    /// Via-Portal Exemption) via `calculate_move_cost()`.
    pub sdf_generator: Option<super::super::sdf_generator::SdfGenerator>,
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
        let resolution_nm = constraints.resolution_nm;

        // Extract layer directions from constraints
        let num_layers = constraints.layer_directions.len();
        let layer_directions = (0..num_layers)
            .map(|i| constraints.get_layer_direction(i))
            .collect();

        // Create EntityGraph for spatial queries and component metadata
        let entity_graph = super::super::EntityGraph::new();

        Self {
            bounds,
            constraints,
            layer_directions,
            resolution_nm,
            entity_graph,
            vias: Vec::new(),
            copper_pours: Vec::new(),
            bounding_box_tracker: BoundingBoxTracker::new(),
            area_threshold_nm2: 1_000_000_000_000, // 1mm²
            net_count_threshold: 100,
            is_manhattan: false,           // Default: PCB (Octilinear) mode
            profile_layers: Vec::new(),    // Empty: no stackup info
            layer_z_positions: Vec::new(), // Empty: no stackup info
            substrate_layers: None,
            net_frequencies: FxHashMap::default(),
            partition_grid: None,
            query_store: None,
            route_net_policies: FxHashMap::default(),
            sdf_generator: None,
        }
    }

    /// v0.1.8: Set per-net routing pattern policies.
    ///
    /// Policies are sourced from `route net: NetName: pattern: ...` statements
    /// in the space definition. When a net has a policy with a pattern, the
    /// Steiner decomposition uses `route_net_with_length_constraint` to inject
    /// pattern macro-moves during routing.
    pub fn set_route_net_policies(
        &mut self,
        policies: FxHashMap<crate::netlist::NetId, super::super::routing_patterns::RoutingPattern>,
    ) {
        self.route_net_policies = policies;
    }

    /// Get all vias placed during routing (for drill file export).
    pub fn get_vias(&self) -> &[super::super::types::Via] {
        &self.vias
    }

    /// Clear a voxel (for rip-up and reroute).
    ///
    /// Removes the voxel from the occupied voxels map, making it available
    /// for routing again.
    pub fn clear_voxel(&mut self, _point: Point3D) {
        // Voxel system removed - EntityGraph manages occupancy
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

    /// Configure the router for ASIC (Manhattan) or PCB (Octilinear) mode.
    ///
    /// When `is_manhattan` is true, multi-layer via transitions are unrolled
    /// into layer-by-layer buried vias with intermediate landing pads.
    ///
    /// # Arguments
    /// * `is_manhattan` - True for ASIC (Manhattan), false for PCB (Octilinear)
    /// * `profile_layers` - Ordered layer names (bottom-to-top) from the stackup
    /// * `layer_z_positions` - Z start positions (bottom) in nm for each layer
    pub fn set_profile_mode(
        &mut self,
        is_manhattan: bool,
        profile_layers: Vec<String>,
        layer_z_positions: Vec<i64>,
    ) {
        self.is_manhattan = is_manhattan;
        self.profile_layers = profile_layers;
        self.layer_z_positions = layer_z_positions;
    }

    /// v0.1.7: Set substrate layers and net frequencies for SI-aware routing.
    ///
    /// Call this before `route_space()` to enable high-speed nets to avoid
    /// reference-plane voids.
    pub fn set_substrate_context(
        &mut self,
        substrate_layers: Vec<crate::geometry_router::substrate_types::SubstrateLayer>,
        net_frequencies: FxHashMap<crate::netlist::NetId, f64>,
    ) {
        self.substrate_layers = Some(substrate_layers);
        self.net_frequencies = net_frequencies;
    }

    // =========================================================================
    // v0.1.7 Minkowski Obstacle Inflation Integration (Section 1.2)
    // =========================================================================

    /// v0.1.7: Check if a net is high-speed (≥1 GHz) based on stored frequencies.
    pub fn is_high_speed_net(&self, net_id: crate::netlist::NetId) -> bool {
        self.net_frequencies
            .get(&net_id)
            .map_or(false, |&freq| freq >= 1_000_000_000.0)
    }

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
        explicit_segments: Option<&[(crate::netlist::NetId, Vec<Point3D>)]>,
        obstacle_bboxes: &[crate::geometry::BoundingBox],
        substrate_layers: Option<&[crate::geometry_router::substrate_types::SubstrateLayer]>,
        net_frequencies: &FxHashMap<crate::netlist::NetId, f64>,
    ) -> Result<super::super::types::RouteResult, super::super::types::RoutingError> {
        // Store substrate context on self so route_net/route_net_global can access it
        if let Some(sl) = substrate_layers {
            self.substrate_layers = Some(sl.to_vec());
        }
        self.net_frequencies = net_frequencies.clone();

        // Build the Entity Graph from VoxelGrid component metadata.
        // This populates the SceneGraph + R*-tree spatial index for vector-first
        // collision queries that the router can use alongside VoxelGrid.
        self.build_entity_graph();

        // v0.1.8: Build SDF generator for Leap-Frog A* routing.
        // The SDF enables the router to skip empty space during pathfinding,
        // and provides the distance field needed by calculate_move_cost() for
        // guardrails (R25 non-routable layers, Interior Lockout, Via-Portal Exemption).
        {
            let x_size = ((grid_bbox.max.x - grid_bbox.min.x) / self.resolution_nm).max(1) as usize;
            let y_size = ((grid_bbox.max.y - grid_bbox.min.y) / self.resolution_nm).max(1) as usize;
            let z_size = ((grid_bbox.max.z - grid_bbox.min.z) / self.resolution_nm).max(1) as usize;
            let mut sdf = super::super::sdf_generator::SdfGenerator::new(
                x_size, y_size, z_size,
                super::super::sdf_generator::VoxelSize {
                    x_nm: self.resolution_nm,
                    y_nm: self.resolution_nm,
                    z_nm: self.resolution_nm,
                },
                0, // substrate_height_nm
            );
            for meta in self.entity_graph.get_component_metadata() {
                sdf.register_component(meta.clone());
            }
            self.sdf_generator = Some(sdf);
        }

        // Create a coarse PartitionGrid before routing branches.
        // This enables topological routing with partition-guided pathfinding.
        let track_pitch = self.resolution_nm;
        let max_clearance = self.constraints.fabrication.as_ref()
            .map(|fab| fab.min_trace_spacing_nm)
            .unwrap_or(200_000);
        let partition = super::super::partition::PartitionGrid::new(
            *grid_bbox,
            10_000_000,
            10_000_000,
            track_pitch,
            max_clearance,
        );
        self.partition_grid = Some(partition);

        // v0.1.7: If explicit segments are provided (Chain-Link mode), route them first
        // and bypass Steiner logic for these specific paths.
        let mut result = if let Some(segments) = explicit_segments {
            self.route_all_nets_explicit_global(segments)?
        } else {
            super::super::types::RouteResult::new()
        };

        let width = grid_bbox.max.x - grid_bbox.min.x;
        let height = grid_bbox.max.y - grid_bbox.min.y;
        let area_nm2 = width * height;
        let net_count = nets.len();

        if area_nm2 < self.area_threshold_nm2 && net_count < self.net_count_threshold {
            // --- PASS-THROUGH MODE ---
            // eprintln!(
            //     "[ADAPTIVE ROUTER] Pass-Through mode: {} nets, area {:.2} mm² (below thresholds)",
            //     net_count,
            //     area_nm2 as f64 / 1_000_000_000_000.0
            // );
            let steiner_result = self.route_all_nets_steiner(nets, obstacle_bboxes, substrate_layers, net_frequencies)?;
            result.merge(steiner_result);
            Ok(result)
        } else {
            // --- HIERARCHICAL MODE ---
            // eprintln!(
            //     "[ADAPTIVE ROUTER] Hierarchical mode: {} nets, area {:.2} mm² (above thresholds)",
            //     net_count,
            //     area_nm2 as f64 / 1_000_000_000_000.0
            // );
            let hierarchical_result = self.route_hierarchical(
                grid_bbox,
                nets,
                obstacle_bboxes,
                substrate_layers,
                net_frequencies,
            )?;
            result.merge(hierarchical_result);
            Ok(result)
        }
    }

    /// Pass-Through routing: routes all nets in a single pass over the entire board.
    ///
    /// Used for small designs where G-Cell partitioning overhead would exceed
    /// the routing time itself. Multi-pin nets are routed using Steiner
    /// Minimum Tree approximation with dynamic target expansion (T-junctions).
    pub fn route_all_nets_steiner(
        &mut self,
        nets: &FxHashMap<crate::netlist::NetId, Vec<Point3D>>,
        _obstacle_bboxes: &[crate::geometry::BoundingBox],
        _substrate_layers: Option<&[crate::geometry_router::substrate_types::SubstrateLayer]>,
        _net_frequencies: &FxHashMap<crate::netlist::NetId, f64>,
    ) -> Result<super::super::types::RouteResult, super::super::types::RoutingError> {
        // Pass-through: route all nets directly (hierarchical decision is made in route_space)
        self.route_all_nets_steiner_global(nets)
    }

    /// Hierarchical routing: partition into G-Cells, global route, then parallel detailed routing.
    ///
    /// For large designs (SoC-scale), the board is divided into coarse G-Cells.
    /// Cross-cell nets are decomposed into local segments using fast line-casting,
    /// then each G-Cell's segments are routed in parallel via Rayon.
    ///
    /// **Performance**: < 2ms for 128 nets on 100mm² board (vs 35s with A* fallback)
    ///
    /// **Architecture** (from `Docs/v0.1.7/Adaptive-Heuristic.md`):
    /// 1. Partition space into G-Cell tiles
    /// 2. Classify nets as intra-cell or cross-cell
    /// 3. Decompose cross-cell nets into local segments via line-casting
    /// 4. Route each G-Cell's segments in parallel (Rayon)
    /// 5. Stitch localized routes back into a unified result
    fn route_hierarchical(
        &mut self,
        grid_bbox: &crate::geometry::BoundingBox,
        nets: &FxHashMap<crate::netlist::NetId, Vec<Point3D>>,
        _obstacle_bboxes: &[crate::geometry::BoundingBox],
        substrate_layers: Option<&[crate::geometry_router::substrate_types::SubstrateLayer]>,
        net_frequencies: &FxHashMap<crate::netlist::NetId, f64>,
    ) -> Result<super::super::types::RouteResult, super::super::types::RoutingError> {
        use rayon::prelude::*;
        use rustc_hash::FxHashMap;
        use crate::geometry::Point3D;

        // Step 1: Partition board into G-Cell regions
        let cell_size_nm = 10_000_000; // 10mm coarse tiles
        let gcell_grid = super::global_router::GCellGrid::partition(grid_bbox, cell_size_nm);

        // eprintln!(
        //     "[ADAPTIVE ROUTER] Partitioned into {} G-Cells ({}nm tiles)",
        //     gcell_grid.cells.len(),
        //     cell_size_nm
        // );

        let mut cross_cell_nets = FxHashMap::default();
        let mut intra_cell_nets: Vec<FxHashMap<crate::netlist::NetId, Vec<Point3D>>> =
            vec![FxHashMap::default(); gcell_grid.cells.len()];

        // Net classification pass
        for (net_id, pins) in nets {
            let mut cell_indices = rustc_hash::FxHashSet::default();
            for p in pins {
                if let Some(idx) = gcell_grid.get_cell_index_at(p.x, p.y) {
                    cell_indices.insert(idx);
                }
            }

            if cell_indices.len() > 1 {
                cross_cell_nets.insert(*net_id, pins.clone());
            } else if let Some(&idx) = cell_indices.iter().next() {
                intra_cell_nets[idx].insert(*net_id, pins.clone());
            }
        }

        let _intra_count: usize = intra_cell_nets.iter().map(|m| m.len()).sum();
        let _cross_count = cross_cell_nets.len();
        // eprintln!(
        //     "[ADAPTIVE ROUTER] Net classification: {} intra-cell, {} cross-cell",
        //     intra_count, cross_count
        // );

        let mut final_result = super::super::types::RouteResult::new();

        // Step 3: Route cross-cell nets in parallel on the full board.
        // Each net is routed independently (no inter-net collision tracking during parallel phase),
        // then results are merged sequentially to build the occupied_voxels map.
        // This produces straight traces (no G-Cell boundary jogs) with parallel A* speed.
        let t_cross = std::time::Instant::now();
        if !cross_cell_nets.is_empty() {
            let mut sorted_cross: Vec<_> = cross_cell_nets.iter().collect();
            sorted_cross.sort_by_key(|(id, _)| id.0);

            // Phase 3a: Route each cross-cell net in parallel (isolated routers)
            let cross_results: Vec<(
                crate::netlist::NetId,
                Result<super::super::types::RoutedNet, super::super::types::RoutingError>,
            )> = sorted_cross
                .par_iter()
                .map(|(&net_id, pins)| {
                    if pins.len() < 2 {
                        return (
                            net_id,
                            Ok(super::super::types::RoutedNet {
                                net_id,
                                paths: vec![vec![pins[0], pins[0]]],
                                vias: Vec::new(),
                            }),
                        );
                    }

                    // Create an isolated router clone for this net (no shared occupied_voxels)
                    let mut isolated_entity_graph = super::super::EntityGraph::new();
                    // v0.1.7: Propagate component metadata to the isolated router.
                    // This ensures the A* solver "sees" pads as obstacles for Interior Lockout.
                    isolated_entity_graph.copy_metadata_from(&self.entity_graph);

                    let mut isolated = GeometryRouter {
                        bounds: self.bounds,
                        constraints: self.constraints.clone(),
                        layer_directions: self.layer_directions.clone(),
                        resolution_nm: self.resolution_nm,
                        entity_graph: isolated_entity_graph,
                        vias: Vec::new(),
                        copper_pours: self.copper_pours.clone(),
                        bounding_box_tracker: self.bounding_box_tracker.clone(),
                        area_threshold_nm2: self.area_threshold_nm2,
                        net_count_threshold: self.net_count_threshold,
                        is_manhattan: self.is_manhattan,
                        profile_layers: self.profile_layers.clone(),
                        layer_z_positions: self.layer_z_positions.clone(),
                        substrate_layers: self.substrate_layers.clone(),
                        net_frequencies: self.net_frequencies.clone(),
                        partition_grid: None,
                        query_store: None,
                        route_net_policies: self.route_net_policies.clone(),
                        sdf_generator: None, // Isolated router: rebuild SDF if needed
                    };

                    let result = isolated.decompose_net_steiner(net_id, pins);
                    (net_id, result)
                })
                .collect();

            // Phase 3b: Merge results sequentially into final_result + build occupied_voxels
            for (net_id, result) in cross_results {
                let routed = result.map_err(|e| {
                    // eprintln!(
                    //     "[ADAPTIVE ROUTER] Cross-cell net {:?} routing failed: {:?}",
                    //     net_id, e
                    // );
                    e
                })?;

                // Record routed segments canonically in the EntityGraph for subsequent nets
                for segment in &routed.paths {
                    self.entity_graph.register_route(net_id, segment);
                }

                final_result.paths.insert(net_id, routed.paths);
                final_result.vias.extend(routed.vias);
            }

            // eprintln!(
            //     "[ADAPTIVE ROUTER] Cross-cell routing: {} nets routed in parallel ({}ms)",
            //     cross_count,
            //     t_cross.elapsed().as_millis()
            // );
        } else {
            // eprintln!(
            //     "[ADAPTIVE ROUTER] Cross-cell routing: 0 nets ({}ms)",
            //     t_cross.elapsed().as_millis()
            // );
        }

        // Step 4: Route intra-cell nets across G-Cells
        // v0.1.8: When a QueryStore is present, use memoized per-G-cell routing
        // so unchanged G-cells return cached results in <1ms.
        // When no QueryStore is present, use the existing parallel Rayon path.
        let t_intra = std::time::Instant::now();
        if !intra_cell_nets.is_empty() {
            // v0.1.8: Check if memoization is active (QueryStore present)
            if self.query_store.is_some() {
                // --- MEMOIZED SEQUENTIAL PATH ---
                // QueryStore is not Sync, so we route sequentially and memoize per G-Cell.
                let file_id = 0u64; // space-level file ID for query keys
                let mut _cached_count = 0usize;
                let mut _routed_count = 0usize;

                for cell in &gcell_grid.cells {
                    let cell_nets = match intra_cell_nets.get(cell.id) {
                        Some(nets) => nets,
                        None => continue,
                    };

                    let gcell_id = cell.id as u32;
                    let query_id = super::super::query_engine::make_query_id(
                        super::super::query_engine::QueryType::RouteGcell,
                        file_id,
                        &[gcell_id as u64],
                    );

                    // Check if this G-cell's routing result is already cached
                    let is_cached = self
                        .query_store
                        .as_ref()
                        .unwrap()
                        .get_result(query_id)
                        .is_some();

                    if is_cached {
                        _cached_count += 1;
                        // Result is already in the final_result from a previous compilation.
                        // Skip re-routing — this G-cell is unchanged.
                        continue;
                    }

                    // Not cached: route this G-cell and store the result
                    let cell_bbox = &cell.bbox;
                    let mut cell_router = GeometryRouter {
                        bounds: GridBounds::new(
                            cell_bbox.max.x - cell_bbox.min.x,
                            cell_bbox.max.y - cell_bbox.min.y,
                            cell_bbox.max.z - cell_bbox.min.z,
                        ),
                        constraints: self.constraints.clone(),
                        layer_directions: self.layer_directions.clone(),
                        resolution_nm: self.resolution_nm,
                        entity_graph: super::super::EntityGraph::new(),
                        vias: Vec::new(),
                        copper_pours: Vec::new(),
                        bounding_box_tracker: BoundingBoxTracker::new(),
                        area_threshold_nm2: self.area_threshold_nm2,
                        net_count_threshold: self.net_count_threshold,
                        is_manhattan: self.is_manhattan,
                        profile_layers: self.profile_layers.clone(),
                        layer_z_positions: self.layer_z_positions.clone(),
                        substrate_layers: self.substrate_layers.clone(),
                        net_frequencies: self.net_frequencies.clone(),
                        partition_grid: None,
                        query_store: None,
                        route_net_policies: self.route_net_policies.clone(),
                        sdf_generator: None, // Cell router: will be built in route_space
                    };

                    let local_nets: FxHashMap<crate::netlist::NetId, Vec<crate::geometry::Point3D>> =
                        cell_nets
                            .iter()
                            .map(|(&net_id, pins)| {
                                let local_pins: Vec<_> = pins
                                    .iter()
                                    .map(|pin| {
                                        crate::geometry::Point3D::new(
                                            pin.x - cell_bbox.min.x,
                                            pin.y - cell_bbox.min.y,
                                            pin.z - cell_bbox.min.z,
                                        )
                                    })
                                    .collect();
                                (net_id, local_pins)
                            })
                            .collect();

                    cell_router.substrate_layers = substrate_layers.map(|sl| sl.to_vec());
                    cell_router.net_frequencies = net_frequencies.clone();
                    cell_router.entity_graph.copy_metadata_from(&self.entity_graph);

                    match cell_router.route_all_nets_steiner_global(&local_nets) {
                        Ok(local_result) => {
                            // Convert local paths back to global coordinates
                            let mut cell_result = super::super::types::RouteResult::new();
                            for (net_id, local_paths) in &local_result.paths {
                                let global_paths: Vec<Vec<_>> = local_paths
                                    .iter()
                                    .map(|segment| {
                                        segment
                                            .iter()
                                            .map(|pt| {
                                                crate::geometry::Point3D::new(
                                                    pt.x + cell_bbox.min.x,
                                                    pt.y + cell_bbox.min.y,
                                                    pt.z + cell_bbox.min.z,
                                                )
                                            })
                                            .collect()
                                    })
                                    .collect();
                                cell_result.paths.insert(*net_id, global_paths);
                            }
                            cell_result.vias.extend(local_result.vias);

                            // v0.1.8: Store the routed G-cell result in the QueryStore cache
                            let segment_count: usize = cell_result
                                .paths
                                .values()
                                .map(|segs| segs.len())
                                .sum();
                            let hash_input = [
                                file_id.to_le_bytes(),
                                (gcell_id as u64).to_le_bytes(),
                                (segment_count as u64).to_le_bytes(),
                            ]
                            .concat();
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            use std::hash::{Hash, Hasher};
                            hash_input.hash(&mut hasher);
                            let hash_val = hasher.finish();
                            let mut hash_bytes = [0u8; 32];
                            hash_bytes[..8].copy_from_slice(&hash_val.to_le_bytes());

                            let route_result = super::super::query_engine::RouteResult {
                                file_id,
                                gcell_id,
                                segment_count,
                                hash: hash_bytes,
                            };
                            self.query_store.as_mut().unwrap().execute_query(
                                query_id,
                                || super::super::query_engine::QueryResult::RouteGcell(route_result),
                            );

                            _routed_count += 1;
                            final_result.merge(cell_result);
                        }
                        Err(e) => {
                            // eprintln!(
                            //     "[ADAPTIVE ROUTER] G-Cell {} intra-cell routing failed: {:?}",
                            //     cell.id, e
                            // );
                            return Err(e);
                        }
                    }
                }

                // eprintln!(
                //     "[ADAPTIVE ROUTER] Memoized G-Cell routing: {} cached (skipped), {} routed ({}ms)",
                //     cached_count,
                //     routed_count,
                //     t_intra.elapsed().as_millis()
                // );
            } else {
                // --- PARALLEL PATH (existing behavior, no memoization) ---
                let intra_results: Vec<
                    Result<super::super::types::RouteResult, super::super::types::RoutingError>,
                > = gcell_grid
                    .cells
                    .par_iter()
                    .map(|cell| {
                        let cell_nets = match intra_cell_nets.get(cell.id) {
                            Some(nets) => nets,
                            None => return Ok(super::super::types::RouteResult::new()),
                        };

                        let cell_bbox = &cell.bbox;
                        let mut cell_router = GeometryRouter {
                            bounds: GridBounds::new(
                                cell_bbox.max.x - cell_bbox.min.x,
                                cell_bbox.max.y - cell_bbox.min.y,
                                cell_bbox.max.z - cell_bbox.min.z,
                            ),
                            constraints: self.constraints.clone(),
                            layer_directions: self.layer_directions.clone(),
                            resolution_nm: self.resolution_nm,
                            entity_graph: super::super::EntityGraph::new(),
                            vias: Vec::new(),
                            copper_pours: Vec::new(),
                            bounding_box_tracker: BoundingBoxTracker::new(),
                            area_threshold_nm2: self.area_threshold_nm2,
                            net_count_threshold: self.net_count_threshold,
                            is_manhattan: self.is_manhattan,
                            profile_layers: self.profile_layers.clone(),
                            layer_z_positions: self.layer_z_positions.clone(),
                            substrate_layers: self.substrate_layers.clone(),
                            net_frequencies: self.net_frequencies.clone(),
                            partition_grid: None,
                            query_store: None,
                            route_net_policies: self.route_net_policies.clone(),
                            sdf_generator: None, // Cell router: will be built in route_space
                        };

                        let local_nets: FxHashMap<
                            crate::netlist::NetId,
                            Vec<crate::geometry::Point3D>,
                        > = cell_nets
                            .iter()
                            .map(|(&net_id, pins)| {
                                let local_pins: Vec<_> = pins
                                    .iter()
                                    .map(|pin| {
                                        crate::geometry::Point3D::new(
                                            pin.x - cell_bbox.min.x,
                                            pin.y - cell_bbox.min.y,
                                            pin.z - cell_bbox.min.z,
                                        )
                                    })
                                    .collect();
                                (net_id, local_pins)
                            })
                            .collect();

                        // Set up sub-router context (replicate what route_space does before routing)
                        cell_router.substrate_layers = substrate_layers.map(|sl| sl.to_vec());
                        cell_router.net_frequencies = net_frequencies.clone();
                        cell_router.entity_graph.copy_metadata_from(&self.entity_graph);

                        let mut cell_result = super::super::types::RouteResult::new();
                        match cell_router.route_all_nets_steiner_global(&local_nets) {
                            Ok(local_result) => {
                                for (net_id, local_paths) in &local_result.paths {
                                    let global_paths: Vec<Vec<_>> = local_paths
                                        .iter()
                                        .map(|segment| {
                                            segment
                                                .iter()
                                                .map(|pt| {
                                                    crate::geometry::Point3D::new(
                                                        pt.x + cell_bbox.min.x,
                                                        pt.y + cell_bbox.min.y,
                                                        pt.z + cell_bbox.min.z,
                                                    )
                                                })
                                                .collect()
                                        })
                                        .collect();
                                    cell_result.paths.insert(*net_id, global_paths);
                                }
                                cell_result.vias.extend(local_result.vias);
                            }
                            Err(e) => {
                                // eprintln!(
                                //     "[ADAPTIVE ROUTER] G-Cell {} intra-cell routing failed: {:?}",
                                //     cell.id, e
                                // );
                                return Err(e);
                            }
                        }

                        Ok(cell_result)
                    })
                    .collect();

                for res in intra_results {
                    final_result.merge(res?);
                }
            }
        }

        let _total_ms = t_cross.elapsed().as_millis() + t_intra.elapsed().as_millis();
        // eprintln!(
        //     "[ADAPTIVE ROUTER] Hierarchical routing complete: {} nets routed ({}ms cross-cell + {}ms intra-cell = {}ms total)",
        //     final_result.paths.len(),
        //     t_cross.elapsed().as_millis(),
        //     t_intra.elapsed().as_millis(),
        //     total_ms
        // );

        Ok(final_result)
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
        self.entity_graph.add_component_metadata(
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
        self.entity_graph
            .add_component_pin(x_nm, y_nm, z_nm, component_name, pin_name, net);
    }

    /// Build the Entity Graph spatial index from current component metadata.
    ///
    /// This rebuilds the R*-tree spatial index from all placed components.
    /// Call this after all component obstacles have been registered via
    /// `add_component_obstacle()` and before routing begins.
    pub fn build_entity_graph(&mut self) {
        use crate::geometry_router::scene_graph::ComponentStamp;

        // Create a fresh scene graph from the component metadata in entity_graph
        let metadata = self.entity_graph.get_component_metadata().to_vec();
        for (idx, meta) in metadata.iter().enumerate() {
            let width = meta.bbox.max.x - meta.bbox.min.x;
            let height = meta.bbox.max.y - meta.bbox.min.y;
            let stamp = ComponentStamp::rectangle(
                idx,
                meta.component_type.to_string(),
                width,
                height,
            );
            let stamp_id = self.entity_graph.scene_mut().register_stamp(stamp);

            use crate::geometry::transform::FixedTransform2D;
            let transform = FixedTransform2D::from_translation(
                meta.bbox.min.x,
                meta.bbox.min.y,
            );

            let net_bindings: Vec<usize> = Vec::new();
            self.entity_graph.scene_mut().place_instance(stamp_id, transform, net_bindings);
        }

        // Build the R*-tree spatial index from all placed instances
        self.entity_graph.rebuild_spatial_index();
    }
}
