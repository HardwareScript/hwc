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
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    let from_name = crate::ir::routing::helpers::construct_entity_name(&route.from)?;
    let to_name = crate::ir::routing::helpers::construct_entity_name(&route.to)?;

    // PHASE 1: CONSTRAINT MANAGER
    let constraints =
        super::constraints::evaluate_constraints(space, route, symbol_table, profile)?;
    let min_clearance_nm = constraints.min_clearance_nm;
    let current_ma = constraints.current_ma;
    let trace_width_nm = constraints.trace_width_nm;

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

    // Resolve target layer override
    let target_z_nm =
        super::constraints::resolve_target_layer(route, stackup_manager, start_boundary)?;

    // External seeding
    let resolution_nm = space.resolution_nm;
    let mut start_pos = Point3D::new(
        start_boundary.x + (start_dir.0 * resolution_nm),
        start_boundary.y + (start_dir.1 * resolution_nm),
        start_boundary.z,
    );
    let mut goal_pos = Point3D::new(
        goal_boundary.x + (goal_dir.0 * resolution_nm),
        goal_boundary.y + (goal_dir.1 * resolution_nm),
        goal_boundary.z,
    );

    // Seed alignment
    if start_dir.0 != 0 {
        start_pos.y = start_boundary.y;
    } else if start_dir.1 != 0 {
        start_pos.x = start_boundary.x;
    }
    if goal_dir.0 != 0 {
        goal_pos.y = goal_boundary.y;
    } else if goal_dir.1 != 0 {
        goal_pos.x = goal_boundary.x;
    }
    if let Some(z) = target_z_nm {
        start_pos.z = z;
        goal_pos.z = z;
    }

    // PHASE 2: GEOMETRY ROUTER - Net registration
    let net_id = crate::ir::routing::helpers::register_net_for_route(
        space,
        route,
        symbol_table,
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
        "[BOX-MODEL-DEBUG]   Start Boundary: ({}, {}, {})",
        start_boundary.x, start_boundary.y, start_boundary.z
    );
    eprintln!("[BOX-MODEL-DEBUG]   Start Dir: {:?}", start_dir);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Start Seed (Router Start): ({}, {}, {})",
        start_pos.x, start_pos.y, start_pos.z
    );
    eprintln!(
        "[BOX-MODEL-DEBUG]   Goal Boundary: ({}, {}, {})",
        goal_boundary.x, goal_boundary.y, goal_boundary.z
    );
    eprintln!("[BOX-MODEL-DEBUG]   Goal Dir: {:?}", goal_dir);
    eprintln!(
        "[BOX-MODEL-DEBUG]   Goal Seed (Router Goal): ({}, {}, {})",
        goal_pos.x, goal_pos.y, goal_pos.z
    );

    // Resolve material
    let route_z = target_z_nm.unwrap_or(start_pos.z);
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

    let mut path = topo_router
        .route(start_pos, goal_pos, &spatial_index, &board_bounds)
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

    // Boundary stitching
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
        let fixed_z = target_z_nm.or(Some(start_pos.z));
        (refined_path, trace_thickness_nm) = super::geometry::refine_path_z(
            refined_path,
            stackup_manager,
            fixed_z,
            start_pos.z,
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
