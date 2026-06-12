//! Manual routing using waypoint interpolation.

use super::super::conversions::{coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use hwc_engine::{HardwareSpace, Point3D};

/// Route a trace manually using Bresenham interpolation.
///
/// Validates that the first and last waypoints connect to the specified pins.
pub fn route_manual(
    space: &mut HardwareSpace,
    route: &hwc_parser::Route,
    origin: hwc_parser::OriginPoint,
    symbol_table: &crate::SymbolTable,
    eval_context: &hwc_parser::EvaluationContext, // UNIVERSAL CONTEXT
    stackup_manager: &crate::ir::stackup_manager::StackupManager,
    profile: Option<&hwc_parser::ProfileDefinition>,
) -> Result<(), IrError> {
    let ctx = CoordinateContext {
        voxel_size: &space.voxel_size,
        grid_size: &space.grid,
        origin,
        space_dimensions: &space.dimensions,
        symbol_table,
        eval_context,       // Pass the universal context
        bbox_tracker: None, // waypoints don't use anchor references
        stackup_manager,
        profile,
    };
    let waypoints: Vec<Point3D> = route
        .path
        .as_ref()
        .map(|p| p.iter().map(|coord| coordinate_to_point(coord, &ctx)).collect())
        .unwrap_or_default();

    if waypoints.is_empty() {
        return Err(IrError::RoutingError("No waypoints specified".into()));
    }

    // PHASE 1: NET CONNECTIVITY CHECK
    // Validate that first/last waypoints are on the pad edges (within pour bboxes)
    let first_waypoint = waypoints[0];
    let last_waypoint = waypoints[waypoints.len() - 1];

    // Look up pad bboxes for the start and end components (pours and contacts)
    let find_pad_bbox = |comp_name: &str| -> Option<hwc_engine::geometry::BoundingBox> {
        // First check pours (component pads)
        if let Some(bbox) = space.pours.iter()
            .filter(|p| {
                p.device_binding.as_ref()
                    .map(|d| d.device_name.as_str() == comp_name)
                    .unwrap_or(false)
            })
            .filter_map(|p| p.bbox)
            .next()
        {
            return Some(bbox);
        }
        // Then check contacts (vias)
        space.contacts.iter()
            .filter(|c| c.name.as_str() == comp_name)
            .filter_map(|c| c.bbox)
            .next()
    };

    let start_bbox = find_pad_bbox(&route.from.component);
    let end_bbox = find_pad_bbox(&route.to.component);

    // Check that first waypoint is on the start pad's edge
    if let Some(bbox) = &start_bbox {
        if !waypoint_on_pad_edge(first_waypoint, bbox, space.voxel_size.x_nm) {
            return Err(IrError::RoutingError(format!(
                "First waypoint ({}, {}, {}) is not on the pad edge of {}. Pour bbox: min=({}, {}) max=({}, {})",
                first_waypoint.x, first_waypoint.y, first_waypoint.z,
                route.from.component,
                bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y,
            )));
        }
    }

    // Check that last waypoint is on the end pad's edge
    if let Some(bbox) = &end_bbox {
        if !waypoint_on_pad_edge(last_waypoint, bbox, space.voxel_size.x_nm) {
            return Err(IrError::RoutingError(format!(
                "Last waypoint ({}, {}, {}) is not on the pad edge of {}. Pour bbox: min=({}, {}) max=({}, {})",
                last_waypoint.x, last_waypoint.y, last_waypoint.z,
                route.to.component,
                bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y,
            )));
        }
    }

    // PHASE 2: TRACE PLACEMENT
    // v0.1.7: Use the net ID already registered for this route
    let net_id = super::helpers::register_net_for_route(space, route, symbol_table)?;

    // v0.1.7: Resolve material dynamically from the stackup layer
    // This ensures that manual traces merge perfectly with via rings/pours on the same layer.
    let material_name = if let (Some(p), Some(first_wp)) = (profile, waypoints.first()) {
        if let (Some(stackup), Some(layer_name)) = (p.stackup.as_ref(), stackup_manager.get_layer_name_at_z(first_wp.z)) {
            stackup.layers.iter()
                .find(|l| l.name.name == layer_name)
                .map(|l| l.material.to_string())
                .unwrap_or_else(|| "Copper".to_string())
        } else {
            "Copper".to_string()
        }
    } else {
        "Copper".to_string()
    };

    let copper_id = space.material_registry.get_or_register(&material_name);

    // v0.1.7: Create analytic trace for substrate layer realization
    // (manual routes must use the same analytic → substrate pipeline as auto routes)
    let trace_width_nm = if let Some(width_expr) = &route.width {
        super::super::conversions::evaluate_expression_to_nm(width_expr, symbol_table)
            .unwrap_or(200_000)
    } else if let Some(trace) = profile.and_then(|p| p.trace.as_ref()) {
        super::super::conversions::measurement_to_nm(&trace.min_width, symbol_table)
    } else {
        200_000
    };

    let thickness_nm = if let Some(first_wp) = waypoints.first() {
        if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(first_wp.z) {
            stackup_manager.get_thickness_for_layer_index(layer_idx)
        } else {
            space.voxel_size.z_nm
        }
    } else {
        space.voxel_size.z_nm
    };

    let mut segments = Vec::new();
    for window in waypoints.windows(2) {
        segments.push(hwc_engine::LineSegment::new(window[0], window[1]));
    }

    let net_name = space.netlist.get_net(net_id).map(|n| n.name.clone()).unwrap_or_default();
    let analytic_trace = hwc_engine::AnalyticTrace::new(
        net_id,
        trace_width_nm,
        thickness_nm,
        segments,
        copper_id,
        net_name.into(),
    );

    space.add_analytic_route(analytic_trace);

    Ok(())
}

/// Check if a waypoint is on one of the 4 edges of a pad (within tolerance),
/// or inside a circular pour bbox. A waypoint is valid if:
/// 1. It's on the perimeter of the pad bbox (rectangular pads), OR
/// 2. It's inside the pad bbox (circular or irregular pours — being inside means connected)
fn waypoint_on_pad_edge(wp: Point3D, bbox: &hwc_engine::geometry::BoundingBox, tolerance_nm: i64) -> bool {
    // Check Z is within the pad's Z range
    if wp.z < bbox.min.z - tolerance_nm || wp.z > bbox.max.z + tolerance_nm {
        return false;
    }

    // Check if point is inside the bbox (valid for circular pours and any pour where interior = connected)
    let inside_x = wp.x >= bbox.min.x - tolerance_nm && wp.x <= bbox.max.x + tolerance_nm;
    let inside_y = wp.y >= bbox.min.y - tolerance_nm && wp.y <= bbox.max.y + tolerance_nm;
    if inside_x && inside_y {
        return true;
    }

    // Also check perimeter for rectangular pads
    let on_left   = (wp.x - bbox.min.x).abs() <= tolerance_nm && wp.y >= bbox.min.y - tolerance_nm && wp.y <= bbox.max.y + tolerance_nm;
    let on_right  = (wp.x - bbox.max.x).abs() <= tolerance_nm && wp.y >= bbox.min.y - tolerance_nm && wp.y <= bbox.max.y + tolerance_nm;
    let on_bottom = (wp.y - bbox.min.y).abs() <= tolerance_nm && wp.x >= bbox.min.x - tolerance_nm && wp.x <= bbox.max.x + tolerance_nm;
    let on_top    = (wp.y - bbox.max.y).abs() <= tolerance_nm && wp.x >= bbox.min.x - tolerance_nm && wp.x <= bbox.max.x + tolerance_nm;

    on_left || on_right || on_bottom || on_top
}
