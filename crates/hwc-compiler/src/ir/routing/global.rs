//! Global automatic routing for all nets in the netlist.
//!
//! This module implements the top-level "route everything" logic used by the CLI.
//! v0.1.7: Uses `GeometryRouter::route_space()` which selects between Pass-Through
//! (flat) and Hierarchical (G-Cell + Rayon) modes based on net count and board area.

use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::{geometry::Point3D, geometry_router::GridBounds, netlist::NetId, HardwareSpace};
use rustc_hash::FxHashMap;

/// Global automatic router for connecting all pins in the netlist.
pub struct AutoRouter<'a> {
    space: &'a mut HardwareSpace,
    /// Stackup manager for Z-axis resolution
    stackup_manager: &'a crate::ir::stackup_manager::StackupManager,
    /// Active profile definition (for ASIC detection and layer info)
    profile: Option<&'a hwc_parser::ProfileDefinition>,
    /// v0.1.7: Net frequencies in Hz for SI-aware routing (high-speed void avoidance).
    net_frequencies: FxHashMap<NetId, f64>,
}

#[derive(Debug, Clone)]
struct PinInfo {
    position: Point3D,
}

impl<'a> AutoRouter<'a> {
    /// Create a new global automatic router.
    pub fn new(
        space: &'a mut HardwareSpace,
        _symbol_table: &'a crate::SymbolTable,
        stackup_manager: &'a crate::ir::stackup_manager::StackupManager,
        profile: Option<&'a hwc_parser::ProfileDefinition>,
        net_frequencies: FxHashMap<NetId, f64>,
    ) -> Self {
        Self {
            space,
            stackup_manager,
            profile,
            net_frequencies,
        }
    }

    /// Route all nets in the design using the GeometryRouter adaptive pipeline.
    ///
    /// v0.1.7: Replaces the per-net SDF loop with a single call to
    /// `GeometryRouter::route_space()`, which selects between Pass-Through
    /// (flat) and Hierarchical (G-Cell + Rayon) modes based on net count
    /// and board area. The result is converted back to `AnalyticTrace`
    /// primitives for the rest of the pipeline.
    pub fn route_all_nets(&mut self) -> Result<(), IrError> {
        use hwc_engine::geometry::BoundingBox;

        // Phase 1: Analyze component pins and group by net
        let net_pins = self.analyze_nets()?;

        if net_pins.is_empty() {
            return Ok(());
        }

        // Phase 2: Build the nets HashMap required by GeometryRouter::route_space()
        // GeometryRouter expects FxHashMap<NetId, Vec<Point3D>> (all pin coords per net).
        let mut geo_nets: FxHashMap<NetId, Vec<Point3D>> = FxHashMap::default();
        let mut net_id_to_name: FxHashMap<NetId, CompactString> = FxHashMap::default();

        for (net_name, pins) in &net_pins {
            if pins.len() < 2 {
                continue;
            }

            // Get or create net ID
            let net_id = self.find_net_id_for_name(net_name)?;

            // Skip nets that already have manual analytic routes
            if self
                .space
                .analytic_routes
                .iter()
                .any(|r| r.net_id == net_id)
            {
                continue;
            }

            let coords: Vec<Point3D> = pins.iter().map(|p| p.position).collect();
            geo_nets.insert(net_id, coords);
            net_id_to_name.insert(net_id, net_name.clone());
        }

        if geo_nets.is_empty() {
            return Ok(());
        }

        // Phase 3: Collect obstacle bounding boxes from placed components
        let mut obstacle_bboxes: Vec<BoundingBox> = Vec::new();
        for metadata in self.space.voxel_grid.get_component_metadata() {
            obstacle_bboxes.push(metadata.bbox);
        }
        // Also register manual analytic traces as obstacles
        for trace in &self.space.analytic_routes {
            for segment in &trace.segments {
                obstacle_bboxes.push(segment.to_bounding_box(trace.width_nm));
            }
        }

        // Phase 4: Build grid bbox and create GeometryRouter
        let grid_bbox = BoundingBox::new(
            Point3D::new(0, 0, 0),
            Point3D::new(
                self.space.dimensions.width_nm,
                self.space.dimensions.height_nm,
                self.space.dimensions.depth_nm,
            ),
        );

        let grid_bounds = GridBounds::new(
            self.space.dimensions.width_nm,
            self.space.dimensions.height_nm,
            self.space.dimensions.depth_nm,
        );

        let constraints =
            hwc_engine::constraint_manager::ConstraintRulebook::new(self.space.voxel_size.x_nm);

        let mut geo_router = hwc_engine::GeometryRouter::new(grid_bounds, constraints);

        // v0.1.7: Configure profile mode for the router.
        // ASIC profiles use Manhattan angle restriction (layer-by-layer via unrolling).
        // PCB profiles use Octilinear (single through-hole via for multi-layer transitions).
        // In both cases, layer info is needed for via tower unrolling.
        if let Some(profile) = self.profile {
            let is_manhattan = profile.is_asic();
            let profile_layers: Vec<String> = self.stackup_manager.ordered_layers().to_vec();
            if !profile_layers.is_empty() {
                let layer_z_positions: Vec<i64> = profile_layers
                    .iter()
                    .map(|name| self.stackup_manager.get_layer_start_z(name).unwrap_or(0))
                    .collect();
                eprintln!(
                    "[ROUTER] {} mode enabled: {} layers, {} layer Z positions",
                    if is_manhattan { "ASIC" } else { "PCB" },
                    profile_layers.len(),
                    layer_z_positions.len()
                );
                geo_router.set_profile_mode(is_manhattan, profile_layers, layer_z_positions);
            }
        }

        // Register component obstacles and pins with the GeometryRouter
        for metadata in self.space.voxel_grid.get_component_metadata() {
            geo_router.add_component_obstacle(
                metadata.bbox,
                metadata.material,
                metadata.name.clone(),
                metadata.component_type.clone(),
            );
        }
        // v0.1.7 Boundary-Docking: Also register pour bboxes as obstacles.
        // Many pads (especially connectors) have pour substrate layers but no
        // component_metadata, so the A* lockout was invisible to them.
        for (idx, layer) in self.space.voxel_grid.get_substrate_layers().iter().enumerate() {
            if layer.layer_type == hwc_engine::voxel_grid::SubstrateLayerType::Pour {
                let name: compact_str::CompactString =
                    format!("pour_{}", idx).into();
                geo_router.add_component_obstacle(
                    layer.bbox,
                    1, // Copper material
                    name,
                    "Pour".into(),
                );
            }
        }
        for pin in self.space.voxel_grid.get_component_pins() {
            geo_router.add_component_pin(
                pin.x_nm,
                pin.y_nm,
                pin.z_nm,
                pin.component_name.clone(),
                pin.pin_name.clone(),
                pin.net.clone(),
            );
        }

        // Phase 5: Route all nets via GeometryRouter (adaptive mode selection)
        let t_route = std::time::Instant::now();
        let substrate_layers = self.space.voxel_grid.get_substrate_layers();
        let has_substrate = !substrate_layers.is_empty();
        match geo_router.route_space(
            &grid_bbox,
            &geo_nets,
            &obstacle_bboxes,
            if has_substrate { Some(substrate_layers) } else { None },
            &self.net_frequencies,
            Some(&self.space.voxel_grid), // v0.1.7: Enable Strict Interior Lockout
        ) {
            Ok(result) => {
                eprintln!(
                    "[ROUTER] GeometryRouter complete: {} nets routed, {} vias placed ({}ms)",
                    result.paths.len(),
                    result.vias.len(),
                    t_route.elapsed().as_millis()
                );

                // Convert RouteResult paths back to AnalyticTrace primitives
                // v0.1.7: Resolve copper thickness from the stackup for the routing layer.
                let trace_thickness_nm = {
                    let default_thickness = self.space.voxel_size.z_nm;
                    let sample_z = result
                        .paths
                        .values()
                        .next()
                        .and_then(|segments| segments.first())
                        .and_then(|p| p.first())
                        .map(|p| p.z)
                        .unwrap_or(0);
                    self.stackup_manager
                        .get_layer_index_at_z(sample_z)
                        .map(|idx| self.stackup_manager.get_thickness_for_layer_index(idx))
                        .unwrap_or(default_thickness)
                };

                for (net_id, segments) in &result.paths {
                    let net_name = net_id_to_name
                        .get(net_id)
                        .cloned()
                        .unwrap_or_else(|| CompactString::from(format!("net_{}", net_id.raw())));

                    for path in segments {
                        if path.len() < 2 {
                            continue;
                        }

                        // v0.1.7: Grid-Agnostic Z-Resolution via StackupManager
                        let mut refined_path = path.clone();

                        let target_z = {
                            let first_z = refined_path.first().map(|p| p.z).unwrap_or(0);
                            let last_z = refined_path.last().map(|p| p.z).unwrap_or(0);
                            let first_layer = self.stackup_manager.get_layer_index_at_z(first_z);
                            let last_layer = self.stackup_manager.get_layer_index_at_z(last_z);

                            match (first_layer, last_layer) {
                                (Some(a), Some(b)) if a == b => {
                                    // Both endpoints on same layer — lock entire path to the endpoints' Z plane.
                                    // This preserves surface mounting Z (e.g. 1.27mm) instead of dropping to 
                                    // the layer's bottom boundary (e.g. 1.235mm), avoiding "roof" artifacts.
                                    Some((first_z + last_z) / 2)
                                }
                                (Some(a), _) => {
                                    // Fallback: lock to first point's layer Z
                                    let _unused_a = a;
                                    Some(first_z)
                                }
                                _ => None,
                            }
                        };

                        if let Some(z) = target_z {
                            for point in refined_path.iter_mut() {
                                point.z = z;
                            }
                        } else {
                            // Fallback: per-point refinement
                            for point in refined_path.iter_mut() {
                                if let Some(layer_idx) =
                                    self.stackup_manager.get_layer_index_at_z(point.z)
                                {
                                    point.z = self
                                        .stackup_manager
                                        .get_z_start_nm_for_layer_index(layer_idx);
                                }
                            }
                        }

                        self.register_analytic_route(
                            *net_id,
                            &net_name,
                            refined_path,
                            trace_thickness_nm,
                        )?;
                    }
                }
            }
            Err(e) => {
                return Err(IrError::RoutingError(format!(
                    "GeometryRouter failed: {}",
                    e
                )));
            }
        }

        // Commit all batch routes
        self.space.voxel_grid.commit_route();

        Ok(())
    }

    fn analyze_nets(&self) -> Result<FxHashMap<CompactString, Vec<PinInfo>>, IrError> {
        let mut net_pins: FxHashMap<CompactString, Vec<PinInfo>> = FxHashMap::default();
        let component_pins = self.space.voxel_grid.get_component_pins();

        for pin in component_pins {
            if let Some(net_name) = &pin.net {
                let pin_info = PinInfo {
                    position: Point3D::new(pin.x_nm, pin.y_nm, pin.z_nm),
                };
                net_pins.entry(net_name.clone()).or_default().push(pin_info);
            }
        }

        Ok(net_pins)
    }

    fn find_net_id_for_name(&mut self, name: &str) -> Result<NetId, IrError> {
        if let Some(id) = self.space.netlist.get_net_by_name(name) {
            Ok(id)
        } else {
            let copper_id = self.space.material_registry.get_or_register("Copper");
            Ok(self.space.netlist.add_net(name.into(), 100_000, copper_id))
        }
    }

    fn register_analytic_route(
        &mut self,
        net_id: NetId,
        net_name: &str,
        path: Vec<Point3D>,
        thickness_nm: i64,
    ) -> Result<(), IrError> {
        use hwc_engine::{AnalyticTrace, LineSegment};

        if path.len() < 2 {
            return Ok(());
        }

        // v0.1.7: Strict Boundary-Docking Model
        // The router now handles "docking" perfectly by starting/ending at the boundary.
        // We no longer need to trim traces inside pads because the pathfinder is
        // physically forbidden from entering them.
        let mut segments = Vec::new();
        let mut start = path[0];

        for i in 1..path.len() - 1 {
            let p1 = path[i - 1];
            let p2 = path[i];
            let p3 = path[i + 1];

            let d1x = p2.x - p1.x;
            let d1y = p2.y - p1.y;
            let d1z = p2.z - p1.z;

            let d2x = p3.x - p2.x;
            let d2y = p3.y - p2.y;
            let d2z = p3.z - p2.z;

            let is_collinear = (d1x == 0 && d2x == 0 && d1y == 0 && d2y == 0)
                || (d1x == 0 && d2x == 0 && d1z == 0 && d2z == 0)
                || (d1y == 0 && d2y == 0 && d1z == 0 && d2z == 0);

            if !is_collinear {
                segments.push(LineSegment::new(start, p2));
                start = p2;
            }
        }
        segments.push(LineSegment::new(start, *path.last().unwrap()));

        let copper_id = self.space.material_registry.get_or_register("Copper");
        let trace = AnalyticTrace::new(
            net_id,
            100_000,
            thickness_nm,
            segments,
            copper_id,
            net_name.into(),
        );

        self.space.analytic_routes.push(trace);
        Ok(())
    }
}
