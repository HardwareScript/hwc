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
    pub fn new(space: &'a mut HardwareSpace, symbol_table: &'a SymbolTable) -> Self {
        Self {
            space,
            symbol_table,
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

            for i in 1..pins.len() {
                let goal_pos = pins[i].position;

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

                if let Some(path) = route_net_sdf_accelerated(start_pos, goal_pos, &params, &sdf) {
                    // Register the route as an analytic primitive
                    self.register_analytic_route(net_id, &net_name, path.clone())?;

                    // v0.1.7: Physical Blitting
                    let copper_id = self.space.material_registry.get_or_register("Copper");
                    let engine_router = hwc_engine::Router::new();
                    engine_router.place_trace(
                        &mut self.space.voxel_grid,
                        &self.space.voxel_size,
                        &path,
                        copper_id,
                        net_id.raw(),
                        1
                    ).map_err(|e| IrError::RoutingError(e.to_string()))?;
                }
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

            let d1 = (p2.x - p1.x, p2.y - p1.y, p2.z - p1.z);
            let d2 = (p3.x - p2.x, p3.y - p2.y, p3.z - p2.z);

            if d1 != d2 {
                segments.push(LineSegment::new(start, p2));
                start = p2;
            }
        }
        segments.push(LineSegment::new(start, *path.last().unwrap()));

        let copper_id = self.space.material_registry.get_or_register("Copper");
        let trace = AnalyticTrace::new(
            net_id,
            100_000, // Default width
            segments,
            copper_id,
            net_name.into(),
        );

        self.space.analytic_routes.push(trace);
        Ok(())
    }
}
