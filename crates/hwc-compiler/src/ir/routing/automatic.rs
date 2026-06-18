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
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
) -> Result<(), IrError> {
    use hwc_engine::constraint_manager::{ConstraintManager, LayerDirection, RouteConstraints};
    use hwc_engine::geometry_router::GridBounds;

    eprintln!(
        "[ROUTER] Route automatic: {}.{} → {}.{}",
        route.from.component, route.from.pin, route.to.component, route.to.pin
    );

    // PHASE 1: CONSTRAINT MANAGER
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
            .map_err(IrError::InvalidExpression)?
    } else {
        min_trace_width_nm
    };

    // v0.1.7: Unified Boundary-Docking Model
    // Instead of routing to pin centers, we calculate exact boundary points.
    let (start_boundary, goal_boundary, start_dir, goal_dir) =
        calculate_boundary_points(space, route, trace_width_nm)?;

    // v0.1.7: External A* Seeding
    // To prevent "hooks" inside pads, the A* solver starts one voxel OUTSIDE the pad.
    let voxel_size = space.voxel_size.x_nm;
    let start_pos = hwc_engine::Point3D::new(
        start_boundary.x + (start_dir.0 * voxel_size),
        start_boundary.y + (start_dir.1 * voxel_size),
        start_boundary.z,
    );
    let goal_pos = hwc_engine::Point3D::new(
        goal_boundary.x + (goal_dir.0 * voxel_size),
        goal_boundary.y + (goal_dir.1 * voxel_size),
        goal_boundary.z,
    );

    // PHASE 2: GEOMETRY ROUTER
    // v0.1.7: Register net connectivity in the netlist
    // This ensures both pins share the same logical net ID.
    let net_id = super::helpers::register_net_for_route(space, route, symbol_table)?;
    let net_name = space.netlist.get_net(net_id).unwrap().name.clone();

    println!("[BOX-MODEL-DEBUG] Net: {}", net_name);
    println!("[BOX-MODEL-DEBUG]   Start Boundary: ({}, {}, {})", start_boundary.x, start_boundary.y, start_boundary.z);
    println!("[BOX-MODEL-DEBUG]   Start Dir: {:?}", start_dir);
    println!("[BOX-MODEL-DEBUG]   Start Seed (A* Start): ({}, {}, {})", start_pos.x, start_pos.y, start_pos.z);
    println!("[BOX-MODEL-DEBUG]   Goal Boundary: ({}, {}, {})", goal_boundary.x, goal_boundary.y, goal_boundary.z);
    println!("[BOX-MODEL-DEBUG]   Goal Dir: {:?}", goal_dir);
    println!("[BOX-MODEL-DEBUG]   Goal Seed (A* Goal): ({}, {}, {})", goal_pos.x, goal_pos.y, goal_pos.z);

    let route_constraints = RouteConstraints {
        min_trace_width_nm: trace_width_nm,
        min_clearance_nm,
        max_parallel_length_nm: 10_000_000,
        max_resistance_ohm: 100.0,
        max_current_ma: current_ma,
        impedance_ohm: None,
    };

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

    let exempt_components = [from_component_name.clone(), to_component_name.clone()];

    let routing_params = hwc_engine::geometry_router::RoutingParams {
        net_id,
        constraints: &route_constraints,
        bounds,
        layer_direction,
        voxel_size: space.voxel_size,
        clearance_zones: &clearance_zones,
        occupied_voxels: &occupied_voxels,
        voxel_grid: Some(&space.voxel_grid), // v0.1.7: Enable Strict Interior Lockout
        corridor: None,   // No corridor constraint in compiler automatic routing
        fixed_z_nm: Some(start_pos.z), // v0.1.7: Lock to starting Z plane
        exempt_components: &exempt_components, // v0.1.7: Escape Exemption
        substrate_layers: None, // v0.1.7: No substrate context in basic automatic routing
        is_high_speed_net: false, // v0.1.7: Default to non-high-speed
    };

    eprintln!("[ROUTER] Creating SDF generator...");
    // Create ANALYTIC SDF generator for leap-frog routing
    // This replaces the 10-second BFS with 1-microsecond geometry queries
    let mut sdf = hwc_engine::geometry_router::SdfGenerator::new(
        space.grid.x_cols,
        space.grid.y_rows,
        space.grid.z_layers,
        space.voxel_size,
        0, // v0.1.7: Substrate height = 0 (allow routing anywhere within bounds)
    );

    // Register all placed components for analytic distance calculation
    // NATIVE ARCHITECTURE: component_metadata lives in VoxelGrid where it belongs
    // v0.1.7: Registering full metadata ensures Layer-Aware Keepouts (KOZ) work
    for metadata in space.voxel_grid.get_component_metadata() {
        sdf.register_component(metadata.clone());
    }

    eprintln!("[ROUTER] Starting A* pathfinding...");
    let mut path = hwc_engine::geometry_router::route_net_sdf_accelerated(
        start_pos,
        goal_pos,
        &routing_params,
        &sdf,
    )
    .ok_or_else(|| {
        IrError::RoutingError(format!(
            "No path found from {}.{} to {}.{} (seed_start: {:?}, seed_goal: {:?})",
            route.from.component,
            route.from.pin,
            route.to.component,
            route.to.pin,
            start_pos,
            goal_pos
        ))
    })?;

    // v0.1.7: Boundary Stitching
    // We prepend/append the actual boundary points to the A* path.
    // This ensures the trace connects perfectly to the pad edge.
    path.insert(0, start_boundary);
    path.push(goal_boundary);

    if path.is_empty() {
        return Err(IrError::RoutingError("Empty path generated".into()));
    }

    // **v0.1.7: ANALYTIC ROUTE REGISTRATION (GOD-TIER PARADIGM SHIFT)**
    let (start_pin_id, goal_pin_id) = super::helpers::get_pin_ids(space, route)?;

    let start_pin_name = space.netlist.get_pin(start_pin_id).unwrap().name.clone();
    let goal_pin_name = space.netlist.get_pin(goal_pin_id).unwrap().name.clone();

    // v0.1.7: Grid-Agnostic Z-Resolution
    // We transform the router's voxel-snapped path back into exact physical layer heights
    // using the StackupManager. This eliminates the 21µm "discretization noise".
    let mut refined_path = path.clone();
    let mut trace_thickness_nm = space.voxel_size.z_nm; // Default to voxel size

    if refined_path.len() >= 2 {
        // v0.1.7: When fixed_z_nm is set (planar-locked 2.5D routing), the SDF router
        // already locks all path points to the exact physical Z. Skip Z-refinement entirely
        // to avoid the StackupManager incorrectly remapping Z to a different layer
        // (e.g. z=0 copper → z=35000 FR4 boundary), which creates fake L0→L1 transitions
        // and inflates trace_thickness_nm to the dielectric thickness.
        if let Some(fixed_z) = routing_params.fixed_z_nm {
            // Resolve thickness once from the fixed Z plane
            if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(fixed_z) {
                trace_thickness_nm = stackup_manager.get_thickness_for_layer_index(layer_idx);
            }
            eprintln!(
                "[ROUTER DEBUG] Planar lock: {} points locked to Z={}nm, thickness={}nm",
                refined_path.len(),
                fixed_z,
                trace_thickness_nm
            );
        } else {
            eprintln!(
                "[ROUTER DEBUG] Refining {} path points via StackupManager...",
                refined_path.len()
            );
            for (i, point) in refined_path.iter_mut().enumerate() {
                let old_z = point.z;
                // 1. Identify which PHYSICAL layer this point is in (v0.1.7 Fix: Use StackupManager, not voxel math)
                if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(point.z) {
                    // 2. Resolve the EXACT physical starting height for that layer
                    // This bypasses the coarse voxel grid's multiplication formula.
                    let true_z = stackup_manager.get_z_start_nm_for_layer_index(layer_idx);

                    // v0.1.7: Extract physical thickness for this layer to prevent "wobbly" 3D meshes
                    trace_thickness_nm = stackup_manager.get_thickness_for_layer_index(layer_idx);

                    // 3. Update the point's Z to the physical truth
                    point.z = true_z;

                    if old_z != true_z {
                        eprintln!(
                            "[ROUTER DEBUG]   Point {}: Z shifted from {}nm to {}nm (Layer Index: {})",
                            i, old_z, true_z, layer_idx
                        );
                    }
                } else {
                    eprintln!("[ROUTER WARNING]   Point {}: Z={}nm could not be mapped to any physical layer!", i, point.z);
                }
            }
        }
    }

    // Primitives Over Pixels
    let segments = {
        let mut segs = Vec::new();
        if refined_path.len() >= 2 {
            let mut start = refined_path[0];
            for i in 1..refined_path.len() - 1 {
                let p1 = refined_path[i - 1];
                let p2 = refined_path[i];
                let p3 = refined_path[i + 1];

                // Calculate direction vectors
                let d1x = p2.x - p1.x;
                let d1y = p2.y - p1.y;
                let d1z = p2.z - p1.z;

                let d2x = p3.x - p2.x;
                let d2y = p3.y - p2.y;
                let d2z = p3.z - p2.z;

                // v0.1.7: MANHATTAN COLLINEARITY CHECK (GOD-TIER SIMPLIFICATION)
                let is_collinear = (d1x == 0 && d2x == 0 && d1y == 0 && d2y == 0) || // Z axis
                                   (d1x == 0 && d2x == 0 && d1z == 0 && d2z == 0) || // Y axis
                                   (d1y == 0 && d2y == 0 && d1z == 0 && d2z == 0); // X axis

                // v0.1.7: Filter short perpendicular steps caused by voxel snap
                let seg_len_sq =
                    (p2.x - start.x).pow(2) + (p2.y - start.y).pow(2) + (p2.z - start.z).pow(2);
                let min_seg_len_sq = 200_000i64.pow(2); // 0.2mm minimum segment length
                let is_short = seg_len_sq < min_seg_len_sq;

                if !is_collinear && !is_short {
                    segs.push(hwc_engine::LineSegment::new(start, p2));
                    start = p2;
                }
            }
            segs.push(hwc_engine::LineSegment::new(
                start,
                *refined_path.last().unwrap(),
            ));
        }
        segs
    };

    // v0.1.7 DFM: Add teardrop fillets at trace-to-pad junctions
    // A teardrop is a short, wider segment that tapers from trace_width to pad_width
    // over a transition length, preventing acid traps and mechanical stress points.
    let teardrop_length_nm = 100_000; // 100µm transition zone
    let teardrop_width_nm = trace_width_nm * 2; // 2× trace width at pad junction
    let teardrop_segments = {
        let mut t_segs = Vec::new();
        if refined_path.len() >= 2 {
            let start = refined_path[0];
            let second = refined_path[1];
            let last = *refined_path.last().unwrap();
            let second_to_last = refined_path[refined_path.len() - 2];

            // Start teardrop: short wider segment from pin along first direction
            let dx_start = second.x - start.x;
            let dy_start = second.y - start.y;
            let len_start = ((dx_start * dx_start + dy_start * dy_start) as f64).sqrt();
            if len_start > 0.0 {
                let dir_x = dx_start as f64 / len_start;
                let dir_y = dy_start as f64 / len_start;
                let teardrop_end = hwc_engine::Point3D::new(
                    start.x + (dir_x * teardrop_length_nm as f64) as i64,
                    start.y + (dir_y * teardrop_length_nm as f64) as i64,
                    start.z,
                );
                t_segs.push(hwc_engine::LineSegment::new(start, teardrop_end));
            }

            // Goal teardrop: short wider segment from pin along last direction
            let dx_goal = last.x - second_to_last.x;
            let dy_goal = last.y - second_to_last.y;
            let len_goal = ((dx_goal * dx_goal + dy_goal * dy_goal) as f64).sqrt();
            if len_goal > 0.0 {
                let dir_x = dx_goal as f64 / len_goal;
                let dir_y = dy_goal as f64 / len_goal;
                // v0.1.7: Outward-Only Teardrops
                // We ensure the teardrop starts at the boundary and goes INTO the trace.
                let teardrop_start = hwc_engine::Point3D::new(
                    last.x - (dir_x * teardrop_length_nm as f64) as i64,
                    last.y - (dir_y * teardrop_length_nm as f64) as i64,
                    last.z,
                );
                t_segs.push(hwc_engine::LineSegment::new(teardrop_start, last));
            }
        }
        t_segs
    };

    // Register main trace as analytic primitive
    let analytic_trace = hwc_engine::AnalyticTrace::new(
        net_id,
        trace_width_nm,
        trace_thickness_nm, // v0.1.7: Exact physical thickness
        segments,
        copper_id,
        net_name.clone(),
    );

    space.add_analytic_route(analytic_trace);

    // Register teardrop fillets as wider analytic primitives
    if !teardrop_segments.is_empty() {
        let teardrop_trace = hwc_engine::AnalyticTrace::new(
            net_id,
            teardrop_width_nm,
            trace_thickness_nm, // v0.1.7: Exact physical thickness
            teardrop_segments,
            copper_id,
            net_name.clone(),
        );
        space.add_analytic_route(teardrop_trace);
        eprintln!("[ROUTER] ✓ DFM teardrop fillets added at trace endpoints");
    }

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

/// Calculate boundary points and exit directions for routing.
fn calculate_boundary_points(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
    trace_width_nm: i64,
) -> Result<(hwc_engine::Point3D, hwc_engine::Point3D, (i64, i64), (i64, i64)), IrError> {
    use hwc_engine::geometry_router::port_escape::{
        calculate_rect_escape, CardinalPort, EdgeOffset,
    };

    // Get pin center positions for heuristic direction
    let (start_pin_center, goal_pin_center) = get_pin_positions(space, route)?;

    // v0.1.7: Auto-Port Heuristic (Shortest Path Edge Selection)
    let dx = goal_pin_center.x - start_pin_center.x;
    let dy = goal_pin_center.y - start_pin_center.y;

    let auto_exit_port = if dx.abs() > dy.abs() {
        if dx > 0 { CardinalPort::East } else { CardinalPort::West }
    } else {
        if dy > 0 { CardinalPort::North } else { CardinalPort::South }
    };

    let auto_enter_port = match auto_exit_port {
        CardinalPort::North => CardinalPort::South,
        CardinalPort::South => CardinalPort::North,
        CardinalPort::East => CardinalPort::West,
        CardinalPort::West => CardinalPort::East,
    };

    // Helper to resolve a port+offset spec to an EscapePoint
    let resolve_point = |from_comp: &str, pin: &str, port: CardinalPort, offset: EdgeOffset, z: i64| {
        space.voxel_grid.get_pour_bbox_for_pin(from_comp, pin).map(|bbox| {
            calculate_rect_escape(&bbox, port, offset, trace_width_nm, 0, z)
        })
    };

    let from_comp = super::helpers::construct_component_name(&route.from)?;
    let to_comp = super::helpers::construct_component_name(&route.to)?;

    // Start Escape
    let start_esc = if let Some(exit_escape) = &route.exit_escape {
        let port = match exit_escape.port {
            hwc_parser::CardinalDirection::North => CardinalPort::North,
            hwc_parser::CardinalDirection::South => CardinalPort::South,
            hwc_parser::CardinalDirection::East => CardinalPort::East,
            hwc_parser::CardinalDirection::West => CardinalPort::West,
        };
        // TODO: Handle explicit offset parsing from exit_escape.offset
        resolve_point(&from_comp, &route.from.pin, port, EdgeOffset::Center, start_pin_center.z)
    } else {
        resolve_point(&from_comp, &route.from.pin, auto_exit_port, EdgeOffset::Center, start_pin_center.z)
    }.ok_or_else(|| IrError::RoutingError(format!("Could not resolve boundary for {}.{}", from_comp, route.from.pin)))?;

    // Goal Escape
    let goal_esc = if let Some(enter_escape) = &route.enter_escape {
        let port = match enter_escape.port {
            hwc_parser::CardinalDirection::North => CardinalPort::North,
            hwc_parser::CardinalDirection::South => CardinalPort::South,
            hwc_parser::CardinalDirection::East => CardinalPort::East,
            hwc_parser::CardinalDirection::West => CardinalPort::West,
        };
        resolve_point(&to_comp, &route.to.pin, port, EdgeOffset::Center, goal_pin_center.z)
    } else {
        resolve_point(&to_comp, &route.to.pin, auto_enter_port, EdgeOffset::Center, goal_pin_center.z)
    }.ok_or_else(|| IrError::RoutingError(format!("Could not resolve boundary for {}.{}", to_comp, route.to.pin)))?;

    Ok((start_esc.point, goal_esc.point, start_esc.direction, goal_esc.direction))
}
