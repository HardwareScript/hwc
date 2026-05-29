//! Manual routing using waypoint interpolation.

use super::super::conversions::{coordinate_to_point, CoordinateContext};
use super::super::errors::IrError;
use super::super::units::{format_distance, format_position_mm};
use super::helpers::get_pin_positions;
use hwc_engine::{HardwareSpace, Point3D, Router};

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
    // Validate that waypoints actually connect to the specified pins
    let (start_pin_pos, end_pin_pos) = get_pin_positions(space, route)?;

    let first_waypoint = waypoints[0];
    let last_waypoint = waypoints[waypoints.len() - 1];

    // Calculate distance from first waypoint to source pin
    let start_distance = calculate_distance(first_waypoint, start_pin_pos);

    // Calculate distance from last waypoint to target pin
    let end_distance = calculate_distance(last_waypoint, end_pin_pos);

    // Allow small tolerance for voxel quantization (1 voxel = 100µm typically)
    let tolerance_nm = space.voxel_size.x_nm;

    if start_distance > tolerance_nm {
        return Err(IrError::DisconnectedNet(Box::new(
            crate::ir::errors::DisconnectedNetDetails {
                route_name: format!(
                    "{}.{} to {}.{}",
                    route.from.component, route.from.pin, route.to.component, route.to.pin
                )
                .into(),
                waypoint_type: "first".into(),
                waypoint_pos: format_position_mm(
                    first_waypoint.x,
                    first_waypoint.y,
                    first_waypoint.z,
                ),
                pin_name: format!("{}.{}", route.from.component, route.from.pin).into(),
                pin_pos: format_position_mm(start_pin_pos.x, start_pin_pos.y, start_pin_pos.z),
                distance: format_distance(start_distance),
            },
        )));
    }

    if end_distance > tolerance_nm {
        return Err(IrError::DisconnectedNet(Box::new(
            crate::ir::errors::DisconnectedNetDetails {
                route_name: format!(
                    "{}.{} to {}.{}",
                    route.from.component, route.from.pin, route.to.component, route.to.pin
                )
                .into(),
                waypoint_type: "last".into(),
                waypoint_pos: format_position_mm(last_waypoint.x, last_waypoint.y, last_waypoint.z),
                pin_name: format!("{}.{}", route.to.component, route.to.pin).into(),
                pin_pos: format_position_mm(end_pin_pos.x, end_pin_pos.y, end_pin_pos.z),
                distance: format_distance(end_distance),
            },
        )));
    }

    // PHASE 2: TRACE PLACEMENT
    // v0.1.7: Use the net ID already registered for this route
    let net_id = super::helpers::register_net_for_route(space, route, symbol_table)?;

    // Get Copper material ID from registry
    let copper_id = space.material_registry.get_or_register("Copper");

    let router = Router::new();
    router
        .place_trace(
            &mut space.voxel_grid,
            &space.voxel_size,
            &waypoints,
            copper_id,
            net_id.raw(),
            1,
        )
        .map_err(|e| IrError::RoutingError(e.to_string()))?;

    // Commit the route to make it visible
    space.voxel_grid.commit_route();

    Ok(())
}

/// Calculate 3D Euclidean distance between two points.
fn calculate_distance(p1: Point3D, p2: Point3D) -> i64 {
    let dx = (p1.x - p2.x) as f64;
    let dy = (p1.y - p2.y) as f64;
    let dz = (p1.z - p2.z) as f64;

    ((dx * dx + dy * dy + dz * dz).sqrt()) as i64
}
