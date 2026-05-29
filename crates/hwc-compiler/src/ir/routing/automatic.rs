//! Automatic routing using A* pathfinding.

use super::super::errors::IrError;
use super::helpers::get_pin_positions;
use hwc_engine::HardwareSpace;

/// Route a trace automatically using A* pathfinding.
///
/// Implements the 3-phase routing pipeline:
/// 1. Constraint Manager: Generate geometric constraints from physics
/// 2. Geometry Router: A* pathfinding with Manhattan routing
/// 3. Design Rule Check: Validate physics compliance
pub fn route_automatic(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    symbol_table: &crate::SymbolTable,
) -> Result<(), IrError> {
    use hwc_engine::constraint_manager::{ConstraintManager, LayerDirection, RouteConstraints};
    use hwc_engine::geometry_router::GridBounds;

    eprintln!(
        "[ROUTER] Route automatic: {}.{} → {}.{}",
        route.from.component, route.from.pin, route.to.component, route.to.pin
    );

    // PHASE 1: CONSTRAINT MANAGER
    let (start_pos, goal_pos) = get_pin_positions(space, route)?;
    eprintln!("[ROUTER]   Start pos: {:?}, Goal pos: {:?}", start_pos, goal_pos);

    let voltage_mv = 5000;
    let current_ma = 20;
    let dielectric_strength_kv_mm = 20.0;
    let is_external = true;

    let _constraint_manager = ConstraintManager::new(space.voxel_size.x_nm);

    let min_clearance_nm = hwc_engine::constraint_manager::calculate_clearance_nm(
        voltage_mv,
        dielectric_strength_kv_mm,
        2,
    );

    let min_trace_width_nm =
        hwc_engine::constraint_manager::calculate_trace_width_nm(current_ma, 10, is_external);

    // v0.1.7: Resolve explicit width if provided, otherwise use calculated minimum
    let trace_width_nm = if let Some(width_expr) = &route.width {
        crate::ir::conversions::evaluate_expression_to_nm(width_expr, symbol_table)
            .map_err(|e| IrError::InvalidExpression(e))?
    } else {
        min_trace_width_nm
    };

    let route_constraints = RouteConstraints {
        min_trace_width_nm: trace_width_nm,
        min_clearance_nm,
        max_parallel_length_nm: 10_000_000,
        max_resistance_ohm: 100.0,
        max_current_ma: current_ma,
        impedance_ohm: None,
    };

    // PHASE 2: GEOMETRY ROUTER
    // v0.1.7: Register net connectivity in the netlist
    // This ensures both pins share the same logical net ID.
    let net_id = super::helpers::register_net_for_route(space, route, symbol_table)?;
    let net_name = space.netlist.get_net(net_id).unwrap().name.clone();

    let bounds = GridBounds::new(
        space.dimensions.width_nm,
        space.dimensions.height_nm,
        space.dimensions.depth_nm,
    );

    let layer_direction = LayerDirection::Any;

    // Collect clearance zones and occupied voxels for pathfinding
    use hwc_engine::constraint_manager::ClearanceZone;
    use rustc_hash::FxHashSet;

    let occupied_voxels: FxHashSet<hwc_engine::Point3D> = FxHashSet::default();
    let clearance_zones: Vec<ClearanceZone> = Vec::new();

    // Get Copper material ID from registry
    let copper_id = space.material_registry.get_or_register("Copper");

    // v0.1.7: Identify exempt components (start and goal)
    let from_component_name = super::helpers::construct_component_name(&route.from)?;
    let to_component_name = super::helpers::construct_component_name(&route.to)?;

    let exempt_components = [
        from_component_name.clone(),
        to_component_name.clone(),
    ];

    let routing_params = hwc_engine::geometry_router::RoutingParams {
        net_id,
        constraints: &route_constraints,
        bounds,
        layer_direction,
        voxel_size: space.voxel_size.clone(),
        clearance_zones: &clearance_zones,
        occupied_voxels: &occupied_voxels,
        voxel_grid: None, // Binary Collision Skip disabled - SDF provides 11× speedup already
        corridor: None,   // No corridor constraint in compiler automatic routing
        fixed_z_nm: Some(start_pos.z), // v0.1.7: Lock to starting Z plane
        exempt_components: &exempt_components, // v0.1.7: Escape Exemption
    };

    eprintln!("[ROUTER] Creating SDF generator...");
    // Create ANALYTIC SDF generator for leap-frog routing
    // This replaces the 10-second BFS with 1-microsecond geometry queries
    let mut sdf = hwc_engine::geometry_router::SdfGenerator::new(
        space.grid.x_cols,
        space.grid.y_rows,
        space.grid.z_layers,
        space.voxel_size.clone(), // v0.1.7: Pass full VoxelSize (X, Y, Z)
        0, // v0.1.7: Substrate height = 0 (allow routing anywhere within bounds)
    );

    // Register all placed components for analytic distance calculation
    // NATIVE ARCHITECTURE: component_metadata lives in VoxelGrid where it belongs
    // v0.1.7: Registering full metadata ensures Layer-Aware Keepouts (KOZ) work
    for metadata in space.voxel_grid.get_component_metadata() {
        sdf.register_component(metadata.clone());
    }

    eprintln!("[ROUTER] Starting A* pathfinding...");
    let path = hwc_engine::geometry_router::route_net_sdf_accelerated(
        start_pos,
        goal_pos,
        &routing_params,
        &sdf,
    )
    .ok_or_else(|| {
        IrError::RoutingError(format!(
            "No path found from {}.{} to {}.{} (start: {:?}, goal: {:?})",
            route.from.component,
            route.from.pin,
            route.to.component,
            route.to.pin,
            start_pos,
            goal_pos
        ))
    })?;

    if path.is_empty() {
        return Err(IrError::RoutingError("Empty path generated".into()));
    }

    // **v0.1.7: ANALYTIC ROUTE REGISTRATION (GOD-TIER PARADIGM SHIFT)**
    let (start_pin_id, goal_pin_id) = super::helpers::get_pin_ids(space, route)?;

    let start_pin_name = space.netlist.get_pin(start_pin_id).unwrap().name.clone();
    let goal_pin_name = space.netlist.get_pin(goal_pin_id).unwrap().name.clone();

    // Primitives Over Pixels
    let segments = {
        let mut segs = Vec::new();
        if path.len() >= 2 {
            let mut start = path[0];
            for i in 1..path.len() - 1 {
                let p1 = path[i - 1];
                let p2 = path[i];
                let p3 = path[i + 1];

                // Manhattan check: did the vector direction change?
                let d1 = (p2.x - p1.x, p2.y - p1.y, p2.z - p1.z);
                let d2 = (p3.x - p2.x, p3.y - p2.y, p3.z - p2.z);

                if d1 != d2 {
                    segs.push(hwc_engine::LineSegment::new(start, p2));
                    start = p2;
                }
            }
            segs.push(hwc_engine::LineSegment::new(start, *path.last().unwrap()));
        }
        segs
    };

    // Register as analytic primitive
    let analytic_trace = hwc_engine::AnalyticTrace::new(
        net_id,
        trace_width_nm,
        segments,
        copper_id,
        net_name.clone(),
    );

    space.add_analytic_route(analytic_trace);
    eprintln!("[ROUTER] ✓ Route registered as analytic primitive");

    // Connect both pins to the net (already done in register_net_for_route, but ensure logical binding)
    space.netlist.connect_pin(start_pin_id, net_id);
    space.netlist.connect_pin(goal_pin_id, net_id);

    eprintln!(
        "[ROUTER] ✓ Pins connected: {}.{} ← {} → {}.{}\n",
        from_component_name, start_pin_name, net_name, to_component_name, goal_pin_name
    );

    // PHASE 3: ANALYTIC DESIGN RULE CHECK (v0.1.7 - GOD-TIER)
    //
    // Geometry-based DRC using analytic distance calculations.
    // Nanometer-exact with no voxel discretization artifacts.

    // Extract full component names from route endpoints to exclude them from clearance checks
    // (pins are on component boundaries, so routes will naturally touch their own components)
    let source_component = from_component_name.clone();
    let dest_component = to_component_name.clone();

    // Check only the CURRENT route (the last one added) against all components
    // This avoids false positives from previous routes
    let current_route = space.analytic_routes.last().unwrap();

    let mut violations = Vec::new();
    for (comp_name, comp_bbox) in &space.component_bboxes {
        // Skip source and destination components (pins are on boundaries)
        if comp_name == source_component || comp_name == dest_component {
            continue;
        }

        if !current_route.check_clearance(comp_bbox, min_clearance_nm) {
            // Calculate actual clearance for error reporting
            let half_w = current_route.width_nm / 2;
            let mut min_dist = i64::MAX;

            for seg in &current_route.segments {
                let dist = seg.distance_to_bbox(comp_bbox);
                min_dist = min_dist.min(dist);
            }

            let actual_clearance = min_dist - half_w;
            violations.push((
                current_route.net_name.clone(),
                comp_name.clone(),
                actual_clearance,
            ));
        }
    }

    if !violations.is_empty() {
        let violation_summary = violations
            .iter()
            .map(|(route_name, comp_name, actual_clearance)| {
                format!(
                    "  - Clearance violation: Route '{}' too close to component '{}': {:.3}mm actual, {:.3}mm required",
                    route_name,
                    comp_name,
                    *actual_clearance as f64 / 1_000_000.0,
                    min_clearance_nm as f64 / 1_000_000.0
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        return Err(IrError::RoutingError(format!(
            "Analytic DRC violations detected for route {}:\n{}",
            net_name, violation_summary
        )));
    }

    Ok(())
}
