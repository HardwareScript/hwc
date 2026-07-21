use crate::ir::errors::IrError;
use crate::ir::routing::automatic::select_routable_port_from_resolution;
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
        CardinalPort::North => (0i64, 1i64),   // +Y
        CardinalPort::South => (0i64, -1i64),  // -Y
        CardinalPort::East => (1i64, 0i64),    // +X
        CardinalPort::West => (-1i64, 0i64),   // -X
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
/// This function applies **Dynamic Boundary Resolution** with Zero-Gap Contact Lock
/// to correct for trace width mismatch between cached AccessRegions (computed with
/// PDK min_width) and actual routed trace widths.
///
/// Returns: (start_point, goal_point, start_normal, goal_normal)
pub fn resolve_route_boundary_points(
    space: &HardwareSpace,
    route: &super::super::types::ResolvedRoute,
    trace_width_nm: i64,
) -> Result<(Point3D, Point3D, hwc_engine::geometry_router::connection_interface::Normal2D, hwc_engine::geometry_router::connection_interface::Normal2D), IrError> {
    eprintln!(
        "[BOUNDARY RESOLUTION] Resolving boundary points for net '{}' (width={}nm)",
        route.net_name, trace_width_nm
    );
    eprintln!("  from EntityId: {:?}", route.from);
    eprintln!("  to EntityId: {:?}", route.to);

    // Query entity names from EntityGraph
    let from_entity_data = space
        .entity_graph
        .get_entity_data(route.from)
        .map_err(|_| IrError::UnresolvedEndpoint {
            endpoint: format!("Entity {:?} ({})", route.from, route.net_name),
            span: miette::SourceSpan::from(0),
            help_message: format!(
                "EntityId {:?} not found in EntityGraph.",
                route.from
            ),
        })?;

    let to_entity_data = space
        .entity_graph
        .get_entity_data(route.to)
        .map_err(|_| IrError::UnresolvedEndpoint {
            endpoint: format!("Entity {:?} ({})", route.to, route.net_name),
            span: miette::SourceSpan::from(0),
            help_message: format!(
                "EntityId {:?} not found in EntityGraph.",
                route.to
            ),
        })?;

    eprintln!("  from entity: '{}' (EntityId: {:?})", from_entity_data.name, route.from);
    eprintln!("  to entity: '{}' (EntityId: {:?})", to_entity_data.name, route.to);

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

    eprintln!("  from_interface: {:?}", from_interface);
    eprintln!("  to_interface: {:?}", to_interface);

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
    
    let from_center = Point3D::new(
        (from_entity_data.bbox.min.x + from_entity_data.bbox.max.x) / 2,
        (from_entity_data.bbox.min.y + from_entity_data.bbox.max.y) / 2,
        (from_entity_data.bbox.min.z + from_entity_data.bbox.max.z) / 2,
    );
    let to_center = Point3D::new(
        (to_entity_data.bbox.min.x + to_entity_data.bbox.max.x) / 2,
        (to_entity_data.bbox.min.y + to_entity_data.bbox.max.y) / 2,
        (to_entity_data.bbox.min.z + to_entity_data.bbox.max.z) / 2,
    );
    
    // Retrieve fabrication constraints for clearance calculation
    let clearance_nm = space.fabrication_constraints
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
        space,
        &from_entity_data.name,
        from_center,
        to_center,
        trace_width_nm,
        clearance_nm,
        &from_entity_data.name,
        &to_entity_data.name,
        from_net_id,
        to_net_id,
    )?;
    
    let enter_port = select_routable_port_from_resolution(
        space,
        &to_entity_data.name,
        to_center,
        from_center,
        trace_width_nm,
        clearance_nm,
        &from_entity_data.name,
        &to_entity_data.name,
        from_net_id,
        to_net_id,
    )?;
    
    // Map selected ports directly to their AccessRegions
    let from_region = select_access_region_by_port(
        &from_interface.access_regions,
        exit_port,
        "source"
    )?;
    
    let to_region = select_access_region_by_port(
        &to_interface.access_regions,
        enter_port,
        "destination"
    )?;

    eprintln!("  from_region entry_point: {:?}", from_region.entry_point);
    eprintln!("  from_region corridor: {:?}", from_region.corridor);
    eprintln!("  to_region entry_point: {:?}", to_region.entry_point);
    eprintln!("  to_region corridor: {:?}", to_region.corridor);

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
    let start_point = resolve_boundary_entry(
        from_region.entry_point,
        from_region.normal,
        pdk_min_width_nm,
        trace_width_nm,
    );
    
    let goal_point = resolve_boundary_entry(
        to_region.entry_point,
        to_region.normal,
        pdk_min_width_nm,
        trace_width_nm,
    );

    eprintln!("  ✅ Zero-Gap Contact Lock start: ({},{},{})", start_point.x, start_point.y, start_point.z);
    eprintln!("  ✅ Zero-Gap Contact Lock goal: ({},{},{})", goal_point.x, goal_point.y, goal_point.z);
    eprintln!("  ✅ Start normal: ({},{})", from_region.normal.x, from_region.normal.y);
    eprintln!("  ✅ Goal normal: ({},{})", to_region.normal.x, to_region.normal.y);

    Ok((start_point, goal_point, from_region.normal, to_region.normal))
}

/// Apply dynamic boundary offset scaling with Zero-Gap Contact Lock.
///
/// The entry_point was pre-computed with `default_width_nm`. This function:
/// 1. Reverses the default shift to find the true pad edge
/// 2. Projects outward by EXACTLY (actual_width/2) for perfect 0nm gap contact
///
/// This ensures the trace edge touches the pad edge exactly, with no gap and no overlap.
fn resolve_boundary_entry(
    entry_point: Point3D,
    normal: hwc_engine::geometry_router::connection_interface::Normal2D,
    default_width_nm: i64,
    actual_width_nm: i64,
) -> Point3D {
    const SCALE: i64 = 1_000_000_000;
    
    let default_half_width = default_width_nm / 2;
    let actual_half_width = actual_width_nm / 2;
    
    // 1. Reverse the pre-cached default shift to find the true pad edge coordinate
    //    (Since normal points outward, we subtract the default half-width)
    let edge_x = entry_point.x - (normal.x as i64 * default_half_width) / SCALE;
    let edge_y = entry_point.y - (normal.y as i64 * default_half_width) / SCALE;
    
    // 2. Project outward by EXACTLY the actual trace half-width (Zero-Gap Contact Lock)
    //    This guarantees: trace_outer_edge = pad_edge (0nm gap)
    let corrected_x = edge_x + (normal.x as i64 * actual_half_width) / SCALE;
    let corrected_y = edge_y + (normal.y as i64 * actual_half_width) / SCALE;
    
    eprintln!("    [resolve_boundary_entry] entry=({},{}) normal=({},{}) default_w={} actual_w={}",
        entry_point.x, entry_point.y, normal.x, normal.y, default_width_nm, actual_width_nm);
    eprintln!("      pad_edge=({},{}) trace_centerline=({},{})",
        edge_x, edge_y, corrected_x, corrected_y);
    eprintln!("      trace_outer_edge will be at pad_edge (0nm gap)");
    
    Point3D::new(corrected_x, corrected_y, entry_point.z)
}

/// Resolve pin center positions from a ResolvedRoute by querying the EntityGraph.
pub fn resolve_route_pin_centers(
    space: &HardwareSpace,
    route: &super::super::types::ResolvedRoute,
) -> Result<(Point3D, Point3D), IrError> {
    use hwc_engine::geometry::EntityId;

    let resolve_center = |entity_id: EntityId| -> Result<Point3D, IrError> {
        let data = space.entity_graph.get_entity_data(entity_id).map_err(|_| {
            IrError::UnresolvedEndpoint {
                endpoint: format!("Entity {:?}", entity_id),
                span: miette::SourceSpan::from(0),
                help_message: "EntityId not found in EntityGraph.".into(),
            }
        })?;
        Ok(Point3D::new(
            (data.bbox.min.x + data.bbox.max.x) / 2,
            (data.bbox.min.y + data.bbox.max.y) / 2,
            (data.bbox.min.z + data.bbox.max.z) / 2,
        ))
    };

    let start = resolve_center(route.from)?;
    let goal = resolve_center(route.to)?;
    Ok((start, goal))
}
