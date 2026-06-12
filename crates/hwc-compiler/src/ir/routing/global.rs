//! Global automatic routing for all nets in the netlist.
//!
//! This module implements the top-level "route everything" logic used by the CLI.
//! It uses the SDF-accelerated Leap-Frog router for high performance.

use crate::ir::errors::IrError;
use crate::SymbolTable;
use compact_str::CompactString;
use hwc_engine::{
    geometry::Point3D,
    geometry_router::{route_net_sdf_accelerated, GridBounds, RoutingParams, SdfGenerator},
    netlist::NetId,
    HardwareSpace,
};
use rustc_hash::FxHashMap;

/// Global automatic router for connecting all pins in the netlist.
pub struct AutoRouter<'a> {
    space: &'a mut HardwareSpace,
    /// Symbol table for component definitions and material lookups
    #[allow(dead_code)]
    symbol_table: &'a SymbolTable,
    /// Stackup manager for Z-axis resolution
    stackup_manager: &'a crate::ir::stackup_manager::StackupManager,
}

#[derive(Debug, Clone)]
struct PinInfo {
    #[allow(dead_code)]
    component_name: CompactString,
    #[allow(dead_code)]
    pin_name: CompactString,
    position: Point3D,
}

impl<'a> AutoRouter<'a> {
    /// Create a new global automatic router.
    pub fn new(
        space: &'a mut HardwareSpace,
        symbol_table: &'a SymbolTable,
        stackup_manager: &'a crate::ir::stackup_manager::StackupManager,
    ) -> Self {
        Self {
            space,
            symbol_table,
            stackup_manager,
        }
    }

    /// Route all nets in the design using SDF acceleration.
    pub fn route_all_nets(&mut self) -> Result<(), IrError> {
        // Phase 1: Analyze component pins and group by net
        let net_pins = self.analyze_nets()?;

        if net_pins.is_empty() {
            return Ok(());
        }

        // Phase 2: Create SDF generator and routing params
        let bounds = GridBounds::new(
            self.space.dimensions.width_nm,
            self.space.dimensions.height_nm,
            self.space.dimensions.depth_nm,
        );

        let mut sdf = SdfGenerator::new(
            self.space.grid.x_cols,
            self.space.grid.y_rows,
            self.space.grid.z_layers,
            self.space.voxel_size.clone(), // v0.1.7: Pass full VoxelSize (X, Y, Z)
            0, // v0.1.7: Substrate height = 0
        );

        // Register all placed components for analytic distance calculation
        for metadata in self.space.voxel_grid.get_component_metadata() {
            sdf.register_component(metadata.clone());
        }

        // Phase 2: Obstacle Blitting (v0.1.7)
        // Register manual traces as obstacles for the auto-router.
        // This ensures the butler routes around the custom power rails.
        for trace in &self.space.analytic_routes {
            for segment in &trace.segments {
                let bbox = segment.to_bounding_box(trace.width_nm);
                sdf.register_obstacle_bbox(bbox);
            }
        }

        let constraints =
            hwc_engine::constraint_manager::ConstraintRulebook::new(self.space.voxel_size.x_nm);
        let default_constraints = constraints.get_default_constraints();

        // Phase 3: Route each net
        for (net_name, pins) in net_pins {
            eprintln!("[ROUTER] Analyzing net '{}' with {} pins", net_name, pins.len());
            for (i, pin) in pins.iter().enumerate() {
                eprintln!("[ROUTER]   Pin {}: ({:.3}mm, {:.3}mm, {:.3}mm)", 
                    i, 
                    pin.position.x as f64 / 1_000_000.0, 
                    pin.position.y as f64 / 1_000_000.0, 
                    pin.position.z as f64 / 1_000_000.0);
            }
            if pins.len() < 2 {
                continue;
            }

            // Get net ID for this name
            let net_id = self.find_net_id_for_name(&net_name)?;

            // v0.1.7: Skip nets that already have manual analytic routes
            // This prevents the butler from trying to re-route what the user already placed.
            if self.space.analytic_routes.iter().any(|r| r.net_id == net_id) {
                // println!("[ROUTER] Skipping net '{}' (already has manual trace)", net_name);
                continue;
            }

            // Simple star topology: route from first pin to all others
            let start_pos = pins[0].position;

            // v0.1.7: Get trace width for edge clipping
            let trace_half_width = default_constraints.min_trace_width_nm / 2;
            eprintln!("[ROUTER DEBUG] Trace half-width for clipping: {}nm (min_width={})", trace_half_width, default_constraints.min_trace_width_nm);

            for i in 1..pins.len() {
                let goal_pos = pins[i].position;

                // v0.1.7: Direct route for 2-pin nets on same layer (no router needed)
                let same_z = (start_pos.z - goal_pos.z).abs() < self.space.voxel_size.z_nm;
                let path = if pins.len() == 2 && same_z {
                    // v0.1.7: Clip trace endpoints to copper pad edges, accounting for trace width
                    // Look up the actual copper pour bbox (not the component body bbox)
                    let find_pad_bbox = |comp_name: &str| -> Option<hwc_engine::geometry::BoundingBox> {
                        self.space.pours.iter()
                            .filter(|p| {
                                p.device_binding.as_ref()
                                    .map(|d| d.device_name.as_str() == comp_name)
                                    .unwrap_or(false)
                            })
                            .filter_map(|p| p.bbox)
                            .next()
                    };

                    let start_bbox = find_pad_bbox(&pins[0].component_name)
                        .or_else(|| self.space.component_bboxes.get(&pins[0].component_name).cloned());
                    let goal_bbox = find_pad_bbox(&pins[i].component_name)
                        .or_else(|| self.space.component_bboxes.get(&pins[i].component_name).cloned());

                    let clipped_start = if let Some(bbox) = start_bbox {
                        eprintln!("[ROUTER DEBUG]   Start pad bbox for '{}': min=({}, {}) max=({}, {})", pins[0].component_name, bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y);
                        if goal_pos.x > start_pos.x {
                            // Trace goes right: trace's LEFT edge = pad's RIGHT edge
                            hwc_engine::Point3D::new(bbox.max.x + trace_half_width, start_pos.y, start_pos.z)
                        } else if goal_pos.x < start_pos.x {
                            // Trace goes left: trace's RIGHT edge = pad's LEFT edge
                            hwc_engine::Point3D::new(bbox.min.x - trace_half_width, start_pos.y, start_pos.z)
                        } else if goal_pos.y > start_pos.y {
                            hwc_engine::Point3D::new(start_pos.x, bbox.max.y + trace_half_width, start_pos.z)
                        } else {
                            hwc_engine::Point3D::new(start_pos.x, bbox.min.y - trace_half_width, start_pos.z)
                        }
                    } else {
                        start_pos
                    };

                    let clipped_goal = if let Some(bbox) = goal_bbox {
                        eprintln!("[ROUTER DEBUG]   Goal pad bbox for '{}': min=({}, {}) max=({}, {})", pins[i].component_name, bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y);
                        if start_pos.x < goal_pos.x {
                            // Trace arrives from left: trace's RIGHT edge = goal pad's LEFT edge
                            hwc_engine::Point3D::new(bbox.min.x - trace_half_width, goal_pos.y, goal_pos.z)
                        } else if start_pos.x > goal_pos.x {
                            // Trace arrives from right: trace's LEFT edge = goal pad's RIGHT edge
                            hwc_engine::Point3D::new(bbox.max.x + trace_half_width, goal_pos.y, goal_pos.z)
                        } else if start_pos.y < goal_pos.y {
                            hwc_engine::Point3D::new(goal_pos.x, bbox.min.y - trace_half_width, goal_pos.z)
                        } else {
                            hwc_engine::Point3D::new(goal_pos.x, bbox.max.y + trace_half_width, goal_pos.z)
                        }
                    } else {
                        goal_pos
                    };

                    eprintln!("[ROUTER DEBUG] Direct route: clipped from ({},{}) to ({},{})", 
                        clipped_start.x, clipped_start.y, clipped_goal.x, clipped_goal.y);
                    vec![clipped_start, clipped_goal]
                } else {
                    // v0.1.7: Identify exempt components (start and goal)
                    let exempt_components = [
                        pins[0].component_name.clone(),
                        pins[i].component_name.clone(),
                    ];

                    let params = RoutingParams {
                        net_id,
                        constraints: &default_constraints,
                        bounds,
                        layer_direction: hwc_engine::constraint_manager::LayerDirection::Any,
                        voxel_size: self.space.voxel_size.clone(),
                        clearance_zones: &[],
                        occupied_voxels: &rustc_hash::FxHashSet::default(),
                        voxel_grid: None,
                        corridor: None,
                        fixed_z_nm: Some(start_pos.z), // v0.1.7: Lock to starting Z plane
                        exempt_components: &exempt_components, // v0.1.7: Escape Exemption
                    };

                    match route_net_sdf_accelerated(start_pos, goal_pos, &params, &sdf) {
                        Some(p) => p,
                        None => continue,
                    }
                };

                // v0.1.7: Grid-Agnostic Z-Resolution
                // We transform the router's voxel-snapped path back into exact physical layer heights
                // using the StackupManager. This eliminates the 21µm "discretization noise".
                let mut refined_path = path.clone();
                let mut trace_thickness_nm = self.space.voxel_size.z_nm;

                if refined_path.len() >= 2 {
                    eprintln!("[ROUTER DEBUG] Refining {} path points via StackupManager...", refined_path.len());
                    for (i, point) in refined_path.iter_mut().enumerate() {
                        let old_z = point.z;
                        // 1. Identify which PHYSICAL layer this point is in
                        if let Some(layer_idx) = self.stackup_manager.get_layer_index_at_z(point.z) {
                            // 2. Resolve the EXACT physical starting height for that layer
                            let true_z = self.stackup_manager.get_z_start_nm_for_layer_index(layer_idx);
                            
                            // v0.1.7: Extract physical thickness for this layer
                            trace_thickness_nm = self.stackup_manager.get_thickness_for_layer_index(layer_idx);

                            // 3. Update the point's Z to the physical truth
                            point.z = true_z;
                            
                            if old_z != true_z {
                                eprintln!("[ROUTER DEBUG]   Point {}: Z shifted from {}nm to {}nm (Layer Index: {})", i, old_z, true_z, layer_idx);
                            }
                        }
                    }

                    // v0.1.7: Restore exact pin positions (X/Y) to eliminate voxel snap offset
                    // Only for router paths (3+ points), not direct routes (2 points, already clipped)
                    if refined_path.len() > 2 {
                        let last_idx = refined_path.len() - 1;
                        refined_path[0].x = start_pos.x;
                        refined_path[0].y = start_pos.y;
                        refined_path[last_idx].x = goal_pos.x;
                        refined_path[last_idx].y = goal_pos.y;
                    }
                }

                // Register the route as an analytic primitive
                self.register_analytic_route(net_id, &net_name, refined_path.clone(), trace_thickness_nm)?;
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
                    component_name: pin.component_name.clone(),
                    pin_name: pin.pin_name.clone(),
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
            // If not found, create it (fallback)
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

        let mut segments = Vec::new();
        let mut start = path[0];
        
        for i in 1..path.len() - 1 {
            let p1 = path[i - 1];
            let p2 = path[i];
            let p3 = path[i + 1];

            // Calculate direction vectors
            let d1x = p2.x - p1.x;
            let d1y = p2.y - p1.y;
            let d1z = p2.z - p1.z;
            
            let d2x = p3.x - p2.x;
            let d2y = p3.y - p2.y;
            let d2z = p3.z - p2.z;

            // v0.1.7: MANHATTAN COLLINEARITY CHECK (GOD-TIER SIMPLIFICATION)
            // Three points are collinear in Manhattan routing if they all lie on the same axis.
            let is_collinear = (d1x == 0 && d2x == 0 && d1y == 0 && d2y == 0) || // Z axis
                               (d1x == 0 && d2x == 0 && d1z == 0 && d2z == 0) || // Y axis
                               (d1y == 0 && d2y == 0 && d1z == 0 && d2z == 0);   // X axis

            if !is_collinear {
                segments.push(LineSegment::new(start, p2));
                start = p2;
            }
        }
        segments.push(LineSegment::new(start, *path.last().unwrap()));

        let copper_id = self.space.material_registry.get_or_register("Copper");
        let trace = AnalyticTrace::new(
            net_id,
            100_000, // Default width
            thickness_nm,
            segments,
            copper_id,
            net_name.into(),
        );

        self.space.analytic_routes.push(trace);
        Ok(())
    }
}
