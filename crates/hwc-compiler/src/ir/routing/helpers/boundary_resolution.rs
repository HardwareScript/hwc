use crate::ir::errors::IrError;
use crate::ir::routing::automatic::{select_routable_port_from_resolution, PortSelectionParams};
use hwc_engine::geometry_router::port_escape::CardinalPort;
use hwc_engine::{HardwareSpace, Point3D};

/// Map a CardinalPort to its corresponding AccessRegion.
///
/// v0.1.9: This replaces geometric-only selection with obstacle-aware port mapping.
fn select_access_region_by_port<'a>(
    regions: &'a [hwc_engine::geometry_router::connection_interface::AccessRegion],
    port: CardinalPort,
    label: &str,
) -> Result<&'a hwc_engine::geometry_router::connection_interface::AccessRegion, IrError> {
    // Map the CardinalPort to its unit normal vector
    let target_normal = match port {
        CardinalPort::North => (0i64, 1i64),  // +Y
        CardinalPort::South => (0i64, -1i64), // -Y
        CardinalPort::East => (1i64, 0i64),   // +X
        CardinalPort::West => (-1i64, 0i64),  // -X
    };

    regions
        .iter()
        .find(|region| {
            let (nx, ny) = region.normal.to_unit_direction();
            nx == target_normal.0 && ny == target_normal.1
        })
        .ok_or_else(|| IrError::InvalidRouteExpression {
            expression: format!("{} port selection", label),
            reason: format!("No AccessRegion found matching port {:?}", port),
        })
}

// ============================================================================
// DEPRECATED FUNCTIONS REMOVED (v0.1.9)
// ============================================================================
//
// The following functions were removed to eliminate the "split-brain bug":
//
// 1. select_access_region_by_direction() - Geometric-only port selection based
//    on explicit escape direction specs. Did not consider obstacles.
//
// 2. select_access_region_toward_point() - Geometric-only port selection based
//    on direction vector toward goal. Used dot product scoring but ignored
//    physical obstacle geometry.
//
// WHY REMOVED:
// These functions caused a split-brain bug where:
//   - boundary.rs::select_routable_port() would correctly analyze obstacles
//     and choose North (clear path)
//   - boundary_resolution.rs::select_access_region_toward_point() would override
//     that decision and choose East (blocked by obstacle)
//   - Result: routing failure due to conflicting decisions
//
// REPLACEMENT:
// All port selection now goes through the unified obstacle-aware system:
//   - select_routable_port_from_resolution() in boundary.rs
//   - Uses topological ray-casting to measure actual clearance
//   - Considers both obstacle geometry (70% weight) and geometric alignment (30% weight)
//   - Makes one authoritative decision with full spatial context
//
// ============================================================================

/// Resolve a ResolvedRoute's EntityId endpoints to boundary coordinates with normals.
///
/// v0.1.9: Now uses the Connection Interface Routing (CIR) system with
/// PhysicalInterface → AccessRegions → Boundary Points.
///
/// v0.2.0: Contact-Aware Routing - if an explicit contact exists on a pad,
/// use its position as the routing point instead of the pad boundary edge.
///
/// This function applies **Dynamic Boundary Resolution** with Zero-Gap Contact Lock
/// to correct for trace width mismatch between cached AccessRegions (computed with
/// PDK min_width) and actual routed trace widths.
///
/// Returns: (start_point, goal_point, start_normal, goal_normal)
pub fn resolve_route_boundary_points(
    space: &HardwareSpace,
    route: &super::super::types::ResolvedRoute,
    trace_width_nm: i64,
) -> Result<
    (
        Point3D,
        Point3D,
        hwc_engine::geometry_router::connection_interface::Normal2D,
        hwc_engine::geometry_router::connection_interface::Normal2D,
    ),
    IrError,
> {
    // Query entity names from EntityGraph
    let from_entity_data = space
        .entity_graph
        .get_entity_data(route.from)
        .map_err(|_| IrError::UnresolvedEndpoint {
            endpoint: format!("Entity {:?} ({})", route.from, route.net_name),
            span: miette::SourceSpan::from(0),
            help_message: format!("EntityId {:?} not found in EntityGraph.", route.from),
        })?;

    let to_entity_data =
        space
            .entity_graph
            .get_entity_data(route.to)
            .map_err(|_| IrError::UnresolvedEndpoint {
                endpoint: format!("Entity {:?} ({})", route.to, route.net_name),
                span: miette::SourceSpan::from(0),
                help_message: format!("EntityId {:?} not found in EntityGraph.", route.to),
            })?;

    eprintln!(
        "  from entity: '{}' (EntityId: {:?})",
        from_entity_data.name, route.from
    );
    eprintln!(
        "  to entity: '{}' (EntityId: {:?})",
        to_entity_data.name, route.to
    );

    // Query PhysicalInterface data by entity name
    let from_interface = space
        .entity_graph
        .get_interface_by_entity_name(&from_entity_data.name)
        .ok_or_else(|| IrError::UnresolvedEndpoint {
            endpoint: format!("Entity '{}' ({})", from_entity_data.name, route.net_name),
            span: miette::SourceSpan::from(0),
            help_message: format!(
                "No PhysicalInterface registered for entity '{}'. \
                 Ensure the entity has registered ConnectionInterface metadata.",
                from_entity_data.name
            ),
        })?;

    let to_interface = space
        .entity_graph
        .get_interface_by_entity_name(&to_entity_data.name)
        .ok_or_else(|| IrError::UnresolvedEndpoint {
            endpoint: format!("Entity '{}' ({})", to_entity_data.name, route.net_name),
            span: miette::SourceSpan::from(0),
            help_message: format!(
                "No PhysicalInterface registered for entity '{}'. \
                 Ensure the entity has registered ConnectionInterface metadata.",
                to_entity_data.name
            ),
        })?;

    // Access the pre-computed AccessRegions
    if from_interface.access_regions.is_empty() {
        return Err(IrError::InvalidRouteExpression {
            expression: format!("route from {}", route.net_name),
            reason: format!(
                "Entity '{}' has no AccessRegions. Point geometry may not support routing.",
                from_entity_data.name
            ),
        });
    }
    if to_interface.access_regions.is_empty() {
        return Err(IrError::InvalidRouteExpression {
            expression: format!("route to {}", route.net_name),
            reason: format!(
                "Entity '{}' has no AccessRegions. Point geometry may not support routing.",
                to_entity_data.name
            ),
        });
    }

    // v0.1.9: UNIFIED OBSTACLE-AWARE PORT SELECTION
    // Use the boundary.rs select_routable_port analyzer to choose ports based on
    // actual obstacle geometry, then map those ports directly to AccessRegions.
    // This eliminates the split-brain bug where geometric selection overrode obstacle analysis.

    // v0.2.0 DATABASE-DRIVEN: Query exact Z from layer connection database using route's declared layer.
    // NO FALLBACK to bbox midpoint - if the connection doesn't exist, FAIL LOUDLY.
    // This prevents silent routing errors from misaligned Z coordinates.

    // Determine the routing layer name from the route constraints
    // (In the future, this should come from ResolvedRoute.layer_name)
    let routing_layer = &route.layer_name;

    eprintln!(
        "[BOUNDARY RESOLUTION] Looking up connection points for layer: '{}'",
        routing_layer
    );

    let from_z = space
        .layer_connection_db
        .get_connection_point(&from_entity_data.name, routing_layer)
        .map(|c| {
            eprintln!(
                "[BOUNDARY RESOLUTION]   FROM entity '{}' on layer '{}': Z={}nm",
                from_entity_data.name, routing_layer, c.z_elevation
            );
            c.z_elevation
        })
        .map_err(|e| {
            let registered = space
                .layer_connection_db
                .get_entity_connections(&from_entity_data.name)
                .map(|layers| {
                    layers
                        .iter()
                        .map(|l| l.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "none".to_string());

            IrError::InvalidRouteExpression {
                expression: format!(
                    "route from {} on layer {}",
                    from_entity_data.name, routing_layer
                ),
                reason: format!(
                    "Entity '{}' has no connection point on routing layer '{}'.\n\
                     \n\
                     Registered connections: {}\n\
                     \n\
                     This usually means:\n\
                     1. The entity doesn't span to this layer (check your stackup), or\n\
                     2. The via wasn't properly registered during placement (compiler bug).\n\
                     \n\
                     Database error: {}",
                    from_entity_data.name, routing_layer, registered, e
                ),
            }
        })?;

    let to_z = space
        .layer_connection_db
        .get_connection_point(&to_entity_data.name, routing_layer)
        .map(|c| {
            eprintln!(
                "[BOUNDARY RESOLUTION]   TO entity '{}' on layer '{}': Z={}nm",
                to_entity_data.name, routing_layer, c.z_elevation
            );
            c.z_elevation
        })
        .map_err(|e| {
            let registered = space
                .layer_connection_db
                .get_entity_connections(&to_entity_data.name)
                .map(|layers| {
                    layers
                        .iter()
                        .map(|l| l.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "none".to_string());

            IrError::InvalidRouteExpression {
                expression: format!(
                    "route to {} on layer {}",
                    to_entity_data.name, routing_layer
                ),
                reason: format!(
                    "Entity '{}' has no connection point on routing layer '{}'.\n\
                     \n\
                     Registered connections: {}\n\
                     \n\
                     This usually means:\n\
                     1. The entity doesn't span to this layer (check your stackup), or\n\
                     2. The via wasn't properly registered during placement (compiler bug).\n\
                     \n\
                     Database error: {}",
                    to_entity_data.name, routing_layer, registered, e
                ),
            }
        })?;

    let from_center = Point3D::new(
        (from_entity_data.bbox.min.x + from_entity_data.bbox.max.x) / 2,
        (from_entity_data.bbox.min.y + from_entity_data.bbox.max.y) / 2,
        from_z,
    );
    let to_center = Point3D::new(
        (to_entity_data.bbox.min.x + to_entity_data.bbox.max.x) / 2,
        (to_entity_data.bbox.min.y + to_entity_data.bbox.max.y) / 2,
        to_z,
    );

    // Retrieve fabrication constraints for clearance calculation
    let clearance_nm = space
        .fabrication_constraints
        .as_ref()
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Fabrication constraints required for routing".into(),
            hint: "Add 'trace:' block to profile with min_spacing".into(),
        })?
        .trace
        .min_spacing_nm;

    // v0.1.9.1: Extract net IDs for exemption during port selection
    // This prevents the ray-caster from treating the pads' own geometry as obstacles
    let from_net_id = from_entity_data.net_id;
    let to_net_id = to_entity_data.net_id;

    // Perform obstacle-aware port selection
    let exit_port = select_routable_port_from_resolution(
        &from_entity_data.name,
        PortSelectionParams {
            space,
            start_center: from_center,
            goal_center: to_center,
            trace_width_nm,
            clearance_nm,
            from_component_name: &from_entity_data.name,
            to_component_name: &to_entity_data.name,
        },
        from_net_id,
        to_net_id,
    )?;

    let enter_port = select_routable_port_from_resolution(
        &to_entity_data.name,
        PortSelectionParams {
            space,
            start_center: to_center,
            goal_center: from_center,
            trace_width_nm,
            clearance_nm,
            from_component_name: &from_entity_data.name,
            to_component_name: &to_entity_data.name,
        },
        from_net_id,
        to_net_id,
    )?;

    // Map selected ports directly to their AccessRegions
    let from_region =
        select_access_region_by_port(&from_interface.access_regions, exit_port, "source")?;

    let to_region =
        select_access_region_by_port(&to_interface.access_regions, enter_port, "destination")?;

    // DYNAMIC BOUNDARY RESOLUTION WITH ZERO-GAP CONTACT LOCK:
    // The cached entry_point was computed with PDK min_width during AccessRegion creation.
    // We need to recalculate for the actual trace width.
    //
    // LAW 1: Zero-Gap Contact Lock
    //   The trace centerline is positioned exactly at (boundary ± trace_width/2)
    //   This guarantees the trace edge touches the pad edge with 0nm gap.
    //
    // LAW 2: Mandatory Perpendicular Escape Segment
    //   The router will enforce a perpendicular escape segment along the normal
    //   (implemented in topological_router)

    // Retrieve PDK min_width from fabrication constraints (REQUIRED)
    let pdk_min_width_nm = space.fabrication_constraints
        .as_ref()
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "PDK missing fabrication_constraints for boundary resolution".into(),
            hint: "Add a 'trace:' block to your profile with explicit min_width.\n\nExample:\n  trace:\n    min_width: 180nm".into(),
        })?
        .trace
        .min_width_nm;

    // v0.1.9: Zero-Gap Contact Lock - no escape stub at boundary
    // v0.2.0: Use database-queried Z values, not cached AccessRegion Z
    let start_point = resolve_boundary_entry(
        from_region.entry_point,
        from_region.normal,
        pdk_min_width_nm,
        trace_width_nm,
        from_z, // Use database Z, not entry_point.z
    );

    let goal_point = resolve_boundary_entry(
        to_region.entry_point,
        to_region.normal,
        pdk_min_width_nm,
        trace_width_nm,
        to_z, // Use database Z, not entry_point.z
    );

    // v0.2.0: Contact-Aware Routing Override
    // If an explicit contact exists on the pad that connects to the target layer,
    // use the contact's position instead of the pad boundary edge.
    // This prevents the router from creating duplicate vias at the wrong location.

    let final_start_point = if let Some((contact_x, contact_y)) = find_contact_on_pad(
        space,
        &from_entity_data.bbox,
        start_point.z, // Use the routing layer Z
        from_net_id,
    ) {
        eprintln!(
            "  🔧 CONTACT OVERRIDE: Using explicit contact at ({},{}) instead of pad edge",
            contact_x, contact_y
        );
        Point3D::new(contact_x, contact_y, start_point.z)
    } else {
        start_point
    };

    let final_goal_point = if let Some((contact_x, contact_y)) = find_contact_on_pad(
        space,
        &to_entity_data.bbox,
        goal_point.z, // Use the routing layer Z
        to_net_id,
    ) {
        eprintln!(
            "  🔧 CONTACT OVERRIDE: Using explicit contact at ({},{}) instead of pad edge",
            contact_x, contact_y
        );
        Point3D::new(contact_x, contact_y, goal_point.z)
    } else {
        goal_point
    };

    Ok((
        final_start_point,
        final_goal_point,
        from_region.normal,
        to_region.normal,
    ))
}

/// Apply dynamic boundary offset scaling with Zero-Gap Contact Lock.
///
/// The entry_point was pre-computed with `default_width_nm`. This function:
/// 1. Reverses the default shift to find the true pad edge
/// 2. Projects outward by EXACTLY (actual_width/2) for perfect 0nm gap contact
/// 3. Uses the database-provided Z elevation (v0.2.0) instead of cached entry_point.z
///
/// This ensures the trace edge touches the pad edge exactly, with no gap and no overlap.
fn resolve_boundary_entry(
    entry_point: Point3D,
    normal: hwc_engine::geometry_router::connection_interface::Normal2D,
    default_width_nm: i64,
    actual_width_nm: i64,
    database_z: i64, // v0.2.0: Use database-queried Z, not cached AccessRegion Z
) -> Point3D {
    const SCALE: i64 = 1_000_000_000;

    let default_half_width = default_width_nm / 2;
    let actual_half_width = actual_width_nm / 2;

    // 1. Reverse the pre-cached default shift to find the true pad edge coordinate
    //    (Since normal points outward, we subtract the default half-width)
    let edge_x = entry_point.x - (normal.x as i64 * default_half_width) / SCALE;
    let edge_y = entry_point.y - (normal.y as i64 * default_half_width) / SCALE;

    // 2. Project INWARD by EXACTLY the actual trace half-width (Zero-Gap Contact Lock)
    //    For the trace edge to touch the pad edge, the centerline must be INSIDE by half-width
    //    Since normal points outward, we SUBTRACT to move inward
    let corrected_x = edge_x - (normal.x as i64 * actual_half_width) / SCALE;
    let corrected_y = edge_y - (normal.y as i64 * actual_half_width) / SCALE;

    Point3D::new(corrected_x, corrected_y, database_z) // v0.2.0: Use database Z
}

/// Resolve pin center positions from a ResolvedRoute by querying the EntityGraph.
///
/// v0.2.0 DATABASE-DRIVEN: Uses the layer connection database for Z coordinates.
/// NO FALLBACK to bbox centerline - if the connection doesn't exist, FAIL LOUDLY.
pub fn resolve_route_pin_centers(
    space: &HardwareSpace,
    route: &super::super::types::ResolvedRoute,
) -> Result<(Point3D, Point3D), IrError> {
    use hwc_engine::geometry::EntityId;

    let routing_layer = &route.layer_name;

    let resolve_center = |entity_id: EntityId, label: &str| -> Result<Point3D, IrError> {
        let data = space.entity_graph.get_entity_data(entity_id).map_err(|_| {
            IrError::UnresolvedEndpoint {
                endpoint: format!("Entity {:?}", entity_id),
                span: miette::SourceSpan::from(0),
                help_message: "EntityId not found in EntityGraph.".into(),
            }
        })?;

        // v0.2.0 DATABASE-DRIVEN: Query layer connection database using route's declared layer
        let z = space
            .layer_connection_db
            .get_connection_point(&data.name, routing_layer)
            .map(|c| c.z_elevation)
            .map_err(|e| IrError::InvalidRouteExpression {
                expression: format!("route {} on layer {}", label, routing_layer),
                reason: format!(
                    "Entity '{}' has no connection point on routing layer '{}'.\n\
                     Registered connections: {:?}\n\
                     Database error: {}",
                    data.name,
                    routing_layer,
                    space.layer_connection_db.get_entity_connections(&data.name),
                    e
                ),
            })?;

        Ok(Point3D::new(
            (data.bbox.min.x + data.bbox.max.x) / 2,
            (data.bbox.min.y + data.bbox.max.y) / 2,
            z,
        ))
    };

    let start = resolve_center(route.from, "from")?;
    let goal = resolve_center(route.to, "to")?;
    Ok((start, goal))
}

/// Find an explicit circular contact (via) on a pad that spans to a target Z layer.
///
/// v0.2.0: Contact-Aware Routing - checks if the user has placed an explicit
/// contact on a pad that already provides the layer transition. If found, returns
/// the contact's XY center position so the router can use it instead of creating
/// a new via at the pad edge.
///
/// Returns: Some((x, y)) if a contact exists, None otherwise
fn find_contact_on_pad(
    space: &HardwareSpace,
    pad_bbox: &hwc_engine::geometry::BoundingBox,
    target_z: i64,
    net_id: Option<hwc_engine::netlist::NetId>,
) -> Option<(i64, i64)> {
    use hwc_engine::geometry_router::substrate_types::SubstrateLayerShape;

    let net_id = net_id?; // Return None if no net_id provided
    let net_raw = net_id.raw();

    // Search all substrate layers for circular contacts that:
    // 1. Are on the same net
    // 2. Have XY center within the pad's XY bbox
    // 3. Span vertically to include the target Z layer
    for (idx, layer) in space.entity_graph.get_substrate_layers().iter().enumerate() {
        // Must be on the same net
        if layer.net != hwc_engine::NetId::new(net_raw) {
            continue;
        }

        // Must be circular (contact shape, not rectangular pour)
        let is_circular = matches!(layer.shape, SubstrateLayerShape::Circle { .. });
        if !is_circular {
            continue;
        }

        let layer_center_x = (layer.bbox.min.x + layer.bbox.max.x) / 2;
        let layer_center_y = (layer.bbox.min.y + layer.bbox.max.y) / 2;

        // Check if contact center is within pad's XY bounds
        let within_pad_x = layer_center_x >= pad_bbox.min.x && layer_center_x <= pad_bbox.max.x;
        let within_pad_y = layer_center_y >= pad_bbox.min.y && layer_center_y <= pad_bbox.max.y;

        if !within_pad_x || !within_pad_y {
            continue;
        }

        // Check if contact spans to the target Z layer (with tolerance)
        let contact_spans_target = layer.bbox.min.z <= target_z && layer.bbox.max.z >= target_z;

        if contact_spans_target {
            eprintln!(
                "[CONTACT FOUND] Layer {} at ({},{}) Z={}→{}nm spans target Z={}nm",
                idx, layer_center_x, layer_center_y, layer.bbox.min.z, layer.bbox.max.z, target_z
            );
            return Some((layer_center_x, layer_center_y));
        }
    }

    None
}
