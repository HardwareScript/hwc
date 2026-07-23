//! Main routing pipeline orchestrator.
//!
//! Coordinates the 3-phase automatic routing pipeline:
//! 1. Constraint Manager
//! 2. Geometry Router
//! 3. Design Rule Check

use crate::ir::errors::IrError;
use hwc_engine::{HardwareSpace, Point3D};

/// Route a trace automatically using topological ray-casting.
///
/// Implements the 3-phase routing pipeline:
/// 1. Constraint Manager: Generate geometric constraints from physics
/// 2. Geometry Router: Topological ray-casting with Manhattan routing
/// 3. Design Rule Check: Validate physics compliance
pub fn route_automatic(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext,
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    let from_name = crate::ir::routing::helpers::construct_entity_name(&route.from)?;
    let to_name = crate::ir::routing::helpers::construct_entity_name(&route.to)?;

    // PHASE 1: CONSTRAINT MANAGER
    let constraints =
        super::constraints::evaluate_constraints(space, route, symbol_table, eval_context, profile)?;
    let min_clearance_nm = constraints.min_clearance_nm;
    let current_ma = constraints.current_ma;
    let trace_width_nm = constraints.trace_width_nm;
    let escape_stub_nm = constraints.escape_stub_nm; // v0.1.9: Declarative Escape Policies

    // Boundary point calculation
    let (start_boundary, goal_boundary, start_dir, goal_dir) =
        super::boundary::calculate_boundary_points(space, route, trace_width_nm)?;

    eprintln!("[BOUNDARY DEBUG] Route: {} -> {}", from_name, to_name);
    eprintln!(
        "[BOUNDARY DEBUG]   start_boundary: ({},{},{})",
        start_boundary.x, start_boundary.y, start_boundary.z
    );
    eprintln!(
        "[BOUNDARY DEBUG]   goal_boundary: ({},{},{})",
        goal_boundary.x, goal_boundary.y, goal_boundary.z
    );

    // v0.1.9: Extract normals from interfaces for perpendicular escape routing
    let (start_normal, goal_normal) = {
        // Convert direction tuples to Normal2D
        const SCALE: i32 = 1_000_000_000;
        let start_n = hwc_engine::geometry_router::connection_interface::Normal2D::new(
            (start_dir.0 * SCALE as i64) as i32,
            (start_dir.1 * SCALE as i64) as i32,
        );
        let goal_n = hwc_engine::geometry_router::connection_interface::Normal2D::new(
            (goal_dir.0 * SCALE as i64) as i32,
            (goal_dir.1 * SCALE as i64) as i32,
        );
        (start_n, goal_n)
    };

    // Resolve target layer override
    let target_z_nm =
        super::constraints::resolve_target_layer(route, stackup_manager, start_boundary)?;

    eprintln!("[ROUTING] Using perpendicular escape routing with Zero-Gap Contact Lock");
    eprintln!("[ROUTING]   start_boundary: ({},{},{})", start_boundary.x, start_boundary.y, start_boundary.z);
    eprintln!("[ROUTING]   goal_boundary: ({},{},{})", goal_boundary.x, goal_boundary.y, goal_boundary.z);
    eprintln!("[ROUTING]   start_normal: ({},{})", start_normal.x, start_normal.y);
    eprintln!("[ROUTING]   goal_normal: ({},{})", goal_normal.x, goal_normal.y);

    // PHASE 2: GEOMETRY ROUTER - Net registration
    let net_id = crate::ir::routing::helpers::register_net_for_route(
        space,
        route,
        symbol_table,
        eval_context,
        stackup_manager,
        profile,
        None,
    )?;
    let net_name = space
        .netlist
        .get_net(net_id)
        .ok_or_else(|| IrError::InvalidRouteExpression {
            expression: format!("net ID {}", net_id.raw()),
            reason: "Net not found after registration".into(),
        })?
        .name
        .clone();

    eprintln!("[BOX-MODEL-DEBUG] Net: {}", net_name);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Start Boundary (Contact Point): ({}, {}, {})",
        start_boundary.x, start_boundary.y, start_boundary.z
    );
    eprintln!("[BOX-MODEL-DEBUG]   Start Normal: ({}, {})", start_normal.x, start_normal.y);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Goal Boundary (Contact Point): ({}, {}, {})",
        goal_boundary.x, goal_boundary.y, goal_boundary.z
    );
    eprintln!("[BOX-MODEL-DEBUG]   Goal Normal: ({}, {})", goal_normal.x, goal_normal.y);

    // Resolve material
    let route_z = target_z_nm.unwrap_or(start_boundary.z);
    let copper_id = super::constraints::resolve_material_for_z(
        route_z,
        stackup_manager,
        &space.material_registry,
        profile,
    )?;

    let from_component_name = crate::ir::routing::helpers::construct_entity_name(&route.from)?;
    let to_component_name = crate::ir::routing::helpers::construct_entity_name(&route.to)?;

    // Build spatial index and run topological router
    let topo_router = hwc_engine::geometry_router::TopologicalRouter::new(
        trace_width_nm,
        space.resolution_nm,
        min_clearance_nm,
    );
    let board_bounds = hwc_engine::BoundingBox::new(
        Point3D::new(0, 0, 0),
        Point3D::new(
            space.dimensions.width_nm,
            space.dimensions.height_nm,
            space.dimensions.depth_nm,
        ),
    );

    let spatial_index =
        super::geometry::build_spatial_index(&super::geometry::SpatialIndexConfig {
            space,
            from_component_name: from_component_name.clone(),
            to_component_name: to_component_name.clone(),
        });

    // v0.1.9: Use route_with_perpendicular_escape for Zero-Gap Contact Lock + Mandatory Perpendicular Escape
    let exempt_net_ids = vec![net_id.raw() as usize];
    
    eprintln!("[ROUTING] Calling router with escape_stub={}nm", escape_stub_nm);
    
    let mut path = topo_router
        .route_with_perpendicular_escape(
            start_boundary,
            goal_boundary,
            start_normal,
            goal_normal,
            escape_stub_nm,
            &spatial_index,
            &board_bounds,
            &exempt_net_ids,
        )
        .ok_or_else(|| IrError::NoPathFound {
            net: format!(
                "{} -> {}",
                crate::ir::routing::helpers::endpoint_label(&route.from),
                crate::ir::routing::helpers::endpoint_label(&route.to)
            )
            .into(),
            from_pin: crate::ir::routing::helpers::endpoint_label(&route.from).into(),
            to_pin: crate::ir::routing::helpers::endpoint_label(&route.to).into(),
        })?
        .waypoints;

    eprintln!("[PERPENDICULAR ESCAPE DEBUG] Routed path has {} waypoints", path.len());
    for (i, wp) in path.iter().enumerate().take(4) {
        eprintln!("  path[{}]: ({},{},{})", i, wp.x, wp.y, wp.z);
    }
    if path.len() > 4 {
        eprintln!("  ... and {} more waypoints", path.len() - 4);
    }

    // Note: The perpendicular escape router already includes start_boundary and goal_boundary
    // in the path, so we don't need boundary stitching here.
    // However, the existing pipeline expects to do its own stitching, so we need to
    // remove the router's boundary points and let the pipeline add them back.
    // This maintains compatibility with downstream processing.
    if path.len() >= 2 {
        // Remove first and last (they're the boundary points added by perpendicular escape)
        let intermediate_path: Vec<Point3D> = if path.len() == 2 {
            // Direct connection - keep both points
            path.clone()
        } else {
            // Multi-segment path - extract intermediate points
            path[1..path.len()-1].to_vec()
        };
        path = intermediate_path;
    }

    // Boundary stitching (as before)
    path.insert(0, start_boundary);
    path.push(goal_boundary);

    // Non-routable layer check
    super::geometry::check_non_routable_layers(&path, stackup_manager, profile)?;

    // Global axis alignment
    super::geometry::align_path_to_axis(&mut path, start_boundary, goal_boundary);

    if path.is_empty() {
        return Err(IrError::EmptyRoute {
            net: format!(
                "{} -> {}",
                crate::ir::routing::helpers::endpoint_label(&route.from),
                crate::ir::routing::helpers::endpoint_label(&route.to)
            )
            .into(),
        });
    }

    // Pin ID resolution and Z-refinement
    let (start_pin_id, goal_pin_id) = crate::ir::routing::helpers::get_pin_ids(space, route)?;

    let mut refined_path = path.clone();
    let mut trace_thickness_nm = space.resolution_nm;

    if refined_path.len() >= 2 {
        let fixed_z = target_z_nm.or(Some(start_boundary.z));
        (refined_path, trace_thickness_nm) = super::geometry::refine_path_z(
            refined_path,
            stackup_manager,
            fixed_z,
            start_boundary.z,
            space.resolution_nm,
        )?;
    }

    if trace_thickness_nm == space.resolution_nm && refined_path.len() >= 2 {
        return Err(IrError::InvalidRouteExpression {
            expression: format!(
                "route from {} to {}",
                crate::ir::routing::helpers::endpoint_label(&route.from),
                crate::ir::routing::helpers::endpoint_label(&route.to)
            ),
            reason: format!(
                "Could not resolve trace thickness from stackup at Z={}nm. \
                 Ensure the stackup is properly defined in your PDK profile.",
                refined_path[0].z
            ),
        });
    }

    // Create segments
    let segments = super::geometry::create_segments(
        &refined_path,
        start_boundary,
        goal_boundary,
        target_z_nm,
        trace_width_nm,
        profile,
    )?;

    eprintln!(
        "[SEGMENT DEBUG] Created {} segments from path:",
        segments.len()
    );
    for (i, seg) in segments.iter().enumerate().take(4) {
        eprintln!(
            "  seg[{}]: ({},{},{}) -> ({},{},{})",
            i, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z
        );
    }
    if segments.len() > 4 {
        eprintln!("  ... and {} more segments", segments.len() - 4);
    }

    // Teardrops
    {
        let teardrop_config = hwc_engine::TeardropConfig::class2(trace_width_nm);
        hwc_engine::TeardropEngine::apply_teardrops(hwc_engine::geometry_router::TeardropRequest {
            entity_graph: &space.entity_graph,
            path: &refined_path,
            start_pin: start_boundary,
            goal_pin: goal_boundary,
            trace_width_nm,
            config: &teardrop_config,
            resolution_nm: space.resolution_nm,
            net_handle: hwc_engine::netlist::NetHandle::new(net_id.raw() as u32),
        });
    }

    // Register analytic trace
    let net_actual_current_ma = space
        .netlist
        .get_net(net_id)
        .and_then(|n| n.current_ma)
        .unwrap_or(0.0);

    let analytic_trace = hwc_engine::AnalyticTrace::new(
        net_id,
        hwc_engine::space::CrossSection::new(trace_width_nm, trace_thickness_nm),
        segments.clone(),
        copper_id,
        net_name.clone(),
        hwc_engine::space::CurrentRating::new(net_actual_current_ma, current_ma),
    );

    eprintln!(
        "[ROUTER] Net '{}': {} segments registered (start_z={}, goal_z={}, target_z={:?})",
        net_name,
        analytic_trace.segments.len(),
        start_boundary.z,
        goal_boundary.z,
        target_z_nm
    );

    // PHASE 3: EM/Thermal verification
    super::verification::verify_em_thermal(&super::verification::EmVerificationParams {
        space,
        net_id,
        net_name: &net_name,
        segments: &analytic_trace.segments,
        trace_width_nm,
        trace_thickness_nm,
        current_ma,
        profile,
    })?;

    space.add_analytic_route(analytic_trace);
    space.netlist.connect_pin(start_pin_id, net_id);
    space.netlist.connect_pin(goal_pin_id, net_id);

    // DRC
    super::verification::run_drc(&super::verification::DrcParams {
        space,
        net_name: net_name.clone(),
        from_component: from_component_name,
        to_component: to_component_name,
        min_clearance_nm,
        route_from: &route.from,
        route_to: &route.to,
    })?;

    Ok(())
}
