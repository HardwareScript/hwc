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
        .map(|p| {
            p.iter()
                .map(|coord| {
                    coordinate_to_point(coord, &ctx).map_err(|e| {
                        IrError::CoordinateResolutionFailed {
                            coordinate_str: "manual route waypoint".into(),
                            reason: e,
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    if waypoints.is_empty() {
        return Err(IrError::EmptyRoute {
            net: format!(
                "{} -> {}",
                super::helpers::endpoint_label(&route.from),
                super::helpers::endpoint_label(&route.to)
            )
            .into(),
        });
    }

    // PHASE 1: NET CONNECTIVITY CHECK
    // Validate that first/last waypoints are on the pad edges (within pour bboxes)
    let first_waypoint = waypoints[0];
    let last_waypoint = waypoints[waypoints.len() - 1];

    // Look up pad bboxes for the start and end components (pours and contacts)
    let find_pad_bbox = |comp_name: &str| -> Option<hwc_engine::geometry::BoundingBox> {
        // First check pours (component pads)
        if let Some(bbox) = space
            .pours
            .iter()
            .filter(|p| {
                p.device_binding
                    .as_ref()
                    .map(|d| d.device_name.as_str() == comp_name)
                    .unwrap_or(false)
            })
            .filter_map(|p| p.bbox)
            .next()
        {
            return Some(bbox);
        }
        // Then check contacts (vias)
        space
            .contacts
            .iter()
            .filter(|c| c.name.as_str() == comp_name)
            .filter_map(|c| c.bbox)
            .next()
    };

    let start_comp_name = match &route.from {
        hwc_parser::RouteEndpointSpec::ComponentPin { component_name, .. } => {
            component_name.as_str()
        }
        hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => name.as_str(),
    };
    let end_comp_name = match &route.to {
        hwc_parser::RouteEndpointSpec::ComponentPin { component_name, .. } => {
            component_name.as_str()
        }
        hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => name.as_str(),
    };

    let start_bbox = find_pad_bbox(start_comp_name);
    let end_bbox = find_pad_bbox(end_comp_name);

    // Check that first waypoint is on the start pad's edge
    if let Some(bbox) = &start_bbox {
        if !waypoint_on_pad_edge(first_waypoint, bbox, space.resolution_nm) {
            return Err(IrError::NoPathFound {
                net: format!(
                    "{} -> {}",
                    super::helpers::endpoint_label(&route.from),
                    super::helpers::endpoint_label(&route.to)
                )
                .into(),
                from_pin: super::helpers::endpoint_label(&route.from).into(),
                to_pin: super::helpers::endpoint_label(&route.to).into(),
            });
        }
    }

    let _router = hwc_engine::geometry_router::GeometryRouter::new(
        hwc_engine::geometry_router::GridBounds::new(
            space.dimensions.width_nm,
            space.dimensions.height_nm,
            space.dimensions.depth_nm,
        ),
        hwc_engine::constraint_manager::ConstraintRulebook::new(space.resolution_nm),
        space.material_registry.clone(),
    );

    // Check that last waypoint is on the end pad's edge
    if let Some(bbox) = &end_bbox {
        if !waypoint_on_pad_edge(last_waypoint, bbox, space.resolution_nm) {
            return Err(IrError::NoPathFound {
                net: format!(
                    "{} -> {}",
                    super::helpers::endpoint_label(&route.from),
                    super::helpers::endpoint_label(&route.to)
                )
                .into(),
                from_pin: super::helpers::endpoint_label(&route.from).into(),
                to_pin: super::helpers::endpoint_label(&route.to).into(),
            });
        }
    }

    // PHASE 2: TRACE PLACEMENT
    // v0.1.7: Use the net ID already registered for this route
    let net_id = super::helpers::register_net_for_route(
        space,
        route,
        symbol_table,
        eval_context,
        stackup_manager,
        profile,
        None,
    )?;

    // v0.1.7: Resolve material dynamically from the stackup layer
    // This ensures that manual traces merge perfectly with via rings/pours on the same layer.
    let first_wp_z = waypoints.first().map(|p| p.z).unwrap_or(0);
    let material_name: compact_str::CompactString = (|| -> Option<compact_str::CompactString> {
        let layer_name = stackup_manager.get_layer_name_at_z(first_wp_z)?;
        profile
            .and_then(|p| p.stackup.as_ref())
            .and_then(|stackup| {
                stackup
                    .layers
                    .iter()
                    .find(|l| l.name.name == layer_name)
                    .map(|l| l.material.clone())
            })
    })()
    .ok_or_else(|| IrError::InvalidRouteExpression {
        expression: "manual route".into(),
        reason: format!(
            "Could not resolve material at Z={}nm from stackup",
            first_wp_z
        ),
    })?;

    let copper_id = space
        .material_registry
        .get_id(&material_name)
        .ok_or_else(|| IrError::UndeclaredMaterial {
            material: material_name.clone(),
        })?;

    // v0.1.7: Create analytic trace for substrate layer realization
    // (manual routes must use the same analytic → substrate pipeline as auto routes)
    let trace_width_nm = if let Some(width_expr) = &route.width {
        super::super::conversions::evaluate_expression_to_nm(width_expr, symbol_table, eval_context).map_err(
            |e| IrError::InvalidRouteExpression {
                expression: "route width".into(),
                reason: e.to_string(),
            },
        )?
    } else if let Some(trace) = profile.and_then(|p| p.trace.as_ref()) {
        super::super::conversions::measurement_to_nm(&trace.min_width, symbol_table, eval_context).map_err(
            |e| IrError::InvalidRouteExpression {
                expression: "route width from profile".into(),
                reason: e.to_string(),
            },
        )?
    } else {
        return Err(IrError::MissingAsicConstraint {
            message: "Manual route has no explicit width and no profile trace constraints.".into(),
            hint: "Add 'width: <value>' to the route, or declare 'trace: min_width: <value>' in the profile.".into(),
        });
    };

    let thickness_nm = if let Some(first_wp) = waypoints.first() {
        if let Some(layer_idx) = stackup_manager.get_layer_index_at_z(first_wp.z) {
            stackup_manager.get_thickness_for_layer_index(layer_idx)?
        } else {
            return Err(IrError::InvalidRouteExpression {
                expression: "manual route".into(),
                reason: format!(
                    "Could not resolve trace thickness from stackup at Z={}nm. \
                     Ensure the stackup is properly defined in your profile.",
                    first_wp.z
                ),
            });
        }
    } else {
        return Err(IrError::EmptyRoute {
            net: format!(
                "{} -> {}",
                super::helpers::endpoint_label(&route.from),
                super::helpers::endpoint_label(&route.to)
            )
            .into(),
        });
    };

    // DEBUG: Print waypoints BEFORE creating LineSegments
    eprintln!(
        "[MANUAL WAYPOINTS] Creating segments from {} waypoints",
        waypoints.len()
    );
    for (i, wp) in waypoints.iter().enumerate() {
        eprintln!("  waypoint[{}]: ({},{},{})", i, wp.x, wp.y, wp.z);
    }

    let mut segments = Vec::new();
    for (i, window) in waypoints.windows(2).enumerate() {
        let seg = hwc_engine::LineSegment::new(window[0], window[1]);
        eprintln!(
            "[MANUAL SEGMENT CREATE] seg[{}]: start=({},{},{}), end=({},{},{})",
            i, seg.start.x, seg.start.y, seg.start.z, seg.end.x, seg.end.y, seg.end.z
        );
        segments.push(seg);
    }

    let net_name = space
        .netlist
        .get_net(net_id)
        .map(|n| n.name.clone())
        .unwrap_or_default();

    let current_ma = if let Some(ref ac) = route.current_limit_ac {
        let _rms = crate::ir::conversions::evaluate_expression_to_ma(&ac.rms, symbol_table)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: "current_limit_ac.rms".into(),
                reason: e.to_string(),
            })?;

        crate::ir::conversions::evaluate_expression_to_ma(&ac.peak, symbol_table).map_err(|e| {
            IrError::InvalidRouteExpression {
                expression: "current_limit_ac.peak".into(),
                reason: e.to_string(),
            }
        })?
    } else {
        return Err(IrError::MissingAsicConstraint {
            message: "Route missing required 'current_limit_ac' property.".into(),
            hint: "Add 'current_limit_ac: [rms: <Value>, peak: <Value>]' to the route definition."
                .into(),
        });
    };

    let net_actual_current_ma = space
        .netlist
        .get_net(net_id)
        .and_then(|n| n.current_ma)
        .unwrap_or(0.0);

    // **v0.2.0 STRUCTURAL FIX: Compute layer_z_range for horizontal traces**
    // Find the Z of the first horizontal segment (start.z == end.z) and look up its
    // layer. Traces can have via-stitch segments at the start/end while still being
    // a single-layer route, so we must not require ALL segments to share the same Z.
    let layer_z_range = segments
        .iter()
        .find(|s| s.start.z == s.end.z)
        .and_then(|s| space.find_layer_at_z(s.start.z))
        .map(|layer| (layer.z_bottom, layer.z_top));

    let analytic_trace = hwc_engine::AnalyticTrace::with_layer_z_range(
        net_id,
        hwc_engine::space::CrossSection::new(trace_width_nm, thickness_nm),
        segments,
        copper_id,
        net_name.clone(),
        hwc_engine::space::CurrentRating::new(net_actual_current_ma, current_ma),
        layer_z_range,
    );

    // v0.2.0: Register parent-level route in hierarchical routing database
    // This is the single source of truth for all routing data.
    let from_entity = format!("{}", super::helpers::endpoint_label(&route.from));
    let to_entity = format!("{}", super::helpers::endpoint_label(&route.to));
    
    eprintln!("[ROUTING DB MANUAL] Registering parent route: from='{}', to='{}', net='{}', net_id={:?}",
        from_entity, to_entity, net_name, net_id);
    
    space.routing_database.register_parent_route(
        analytic_trace,
        from_entity.into(),
        to_entity.into(),
    );

    Ok(())
}

/// Check if a waypoint is on one of the 4 edges of a pad (within tolerance),
/// or inside a circular pour bbox. A waypoint is valid if:
/// 1. It's on the perimeter of the pad bbox (rectangular pads), OR
/// 2. It's inside the pad bbox (circular or irregular pours — being inside means connected)
fn waypoint_on_pad_edge(
    wp: Point3D,
    bbox: &hwc_engine::geometry::BoundingBox,
    tolerance_nm: i64,
) -> bool {
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
    let on_left = (wp.x - bbox.min.x).abs() <= tolerance_nm
        && wp.y >= bbox.min.y - tolerance_nm
        && wp.y <= bbox.max.y + tolerance_nm;
    let on_right = (wp.x - bbox.max.x).abs() <= tolerance_nm
        && wp.y >= bbox.min.y - tolerance_nm
        && wp.y <= bbox.max.y + tolerance_nm;
    let on_bottom = (wp.y - bbox.min.y).abs() <= tolerance_nm
        && wp.x >= bbox.min.x - tolerance_nm
        && wp.x <= bbox.max.x + tolerance_nm;
    let on_top = (wp.y - bbox.max.y).abs() <= tolerance_nm
        && wp.x >= bbox.min.x - tolerance_nm
        && wp.x <= bbox.max.x + tolerance_nm;

    on_left || on_right || on_bottom || on_top
}
