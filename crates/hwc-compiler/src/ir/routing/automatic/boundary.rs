//! v0.1.9 CIR Boundary Point Calculation
//!
//! Computes exact boundary points using the Connection Interface Routing (CIR) system.
//! This replaces the legacy direct port_escape calls with interface-aware routing.

use crate::ir::errors::IrError;
use compact_str::CompactString;
use hwc_engine::geometry_router::interface_escape::calculate_interface_escape;
use hwc_engine::geometry_router::port_escape::{CardinalPort, EdgeOffset, NamedPosition};
use hwc_engine::geometry_router::topological_router::types::RayDirection;
use hwc_engine::geometry_router::TopologicalRouter;
use hwc_engine::{HardwareSpace, Point3D};

/// Result of boundary-point calculation: two 3D points (entry/exit) plus the
/// two escape directions expressed as integer (dx, dy) offsets.
pub(crate) type BoundaryPoints = Result<(Point3D, Point3D, (i64, i64), (i64, i64)), IrError>;

/// Port selection result with obstacle analysis
#[derive(Debug)]
struct PortAnalysis {
    port: CardinalPort,
    has_access_region: bool,
    ray_clearance_nm: i64,
    geometric_alignment: f64,
}

impl PortAnalysis {
    /// Compute comprehensive score for port selection
    fn score(&self) -> f64 {
        if !self.has_access_region {
            return f64::NEG_INFINITY;
        }
        
        // Normalize ray clearance to 0-1 range (assume 1mm = full score)
        let clearance_score = (self.ray_clearance_nm as f64 / 1_000_000.0).min(1.0);
        
        // Combine geometric alignment (weight: 0.3) with clearance (weight: 0.7)
        // Prioritize obstacle clearance over pure geometric direction
        (self.geometric_alignment * 0.3) + (clearance_score * 0.7)
    }
}

/// Select an escape port by analyzing obstacles and spatial clearance.
///
/// Uses topological ray-casting to measure actual routing space availability
/// in each cardinal direction, then selects the port with:
/// 1. Valid access region (interface constraint)
/// 2. Maximum clearance from obstacles
/// 3. Reasonable geometric alignment toward goal
fn select_routable_port(
    space: &HardwareSpace,
    endpoint: &hwc_parser::RouteEndpointSpec,
    start_center: Point3D,
    goal_center: Point3D,
    trace_width_nm: i64,
    clearance_nm: i64,
    from_component_name: &CompactString,
    to_component_name: &CompactString,
) -> Result<CardinalPort, IrError> {
    select_routable_port_impl(
        space,
        endpoint,
        start_center,
        goal_center,
        trace_width_nm,
        clearance_nm,
        from_component_name,
        to_component_name,
    )
}

/// Public wrapper for boundary_resolution.rs to call obstacle-aware port selection
/// by entity name (CompactString) instead of RouteEndpointSpec.
///
/// v0.1.9.1: Added net_id exemptions to prevent ray-caster from treating the pad's
/// own geometry as an obstacle during port selection.
pub fn select_routable_port_from_resolution(
    space: &HardwareSpace,
    entity_name: &CompactString,
    start_center: Point3D,
    goal_center: Point3D,
    trace_width_nm: i64,
    clearance_nm: i64,
    from_component_name: &CompactString,
    to_component_name: &CompactString,
    from_net_id: Option<hwc_engine::netlist::NetId>,
    to_net_id: Option<hwc_engine::netlist::NetId>,
) -> Result<CardinalPort, IrError> {
    select_routable_port_core(
        space,
        entity_name,
        start_center,
        goal_center,
        trace_width_nm,
        clearance_nm,
        from_component_name,
        to_component_name,
        from_net_id,
        to_net_id,
    )
}

/// Core implementation of obstacle-aware port selection.
///
/// v0.1.9.1: Extracts net IDs from entity data for exemption during ray-casting.
fn select_routable_port_impl(
    space: &HardwareSpace,
    endpoint: &hwc_parser::RouteEndpointSpec,
    start_center: Point3D,
    goal_center: Point3D,
    trace_width_nm: i64,
    clearance_nm: i64,
    from_component_name: &CompactString,
    to_component_name: &CompactString,
) -> Result<CardinalPort, IrError> {
    let entity_name = match endpoint {
        hwc_parser::RouteEndpointSpec::ComponentPin {
            component_name,
            pin_name,
            ..
        } => CompactString::from(format!("{}.{}", component_name, pin_name)),
        hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => name.clone(),
    };
    
    // v0.1.9.1: Extract net IDs from both source and destination for exemption
    let from_entity_id = hwc_engine::EntityId::from_semantic(
        &format!("space:{}", from_component_name)
    );
    let to_entity_id = hwc_engine::EntityId::from_semantic(
        &format!("space:{}", to_component_name)
    );
    
    let from_net_id = space.entity_graph.get_entity_data(from_entity_id).ok().and_then(|d| d.net_id);
    let to_net_id = space.entity_graph.get_entity_data(to_entity_id).ok().and_then(|d| d.net_id);
    
    select_routable_port_core(
        space,
        &entity_name,
        start_center,
        goal_center,
        trace_width_nm,
        clearance_nm,
        from_component_name,
        to_component_name,
        from_net_id,
        to_net_id,
    )
}

/// Core logic for obstacle-aware port selection (shared by both wrappers).
///
/// v0.1.9.1 BUG FIX: Net exemption propagation to prevent self-collision during port selection.
fn select_routable_port_core(
    space: &HardwareSpace,
    entity_name: &CompactString,
    start_center: Point3D,
    goal_center: Point3D,
    trace_width_nm: i64,
    clearance_nm: i64,
    from_component_name: &CompactString,
    to_component_name: &CompactString,
    from_net_id: Option<hwc_engine::netlist::NetId>,
    to_net_id: Option<hwc_engine::netlist::NetId>,
) -> Result<CardinalPort, IrError> {
    let interface = space
        .entity_graph
        .get_interface_by_entity_name(&entity_name)
        .ok_or_else(|| IrError::InvalidRouteExpression {
            expression: entity_name.to_string(),
            reason: "Entity has no registered interface".into(),
        })?;
    
    let board_bounds = space.entity_graph.total_bounding_box();
    
    // Build spatial index excluding the pad we're analyzing
    // This ensures ray-casting doesn't hit the pad itself as an obstacle
    let spatial_index = super::geometry::build_spatial_index(&super::geometry::SpatialIndexConfig {
        space,
        from_component_name: entity_name.clone(),  // Exclude the pad being analyzed
        to_component_name: if entity_name == from_component_name {
            to_component_name.clone()  // If analyzing source, also exclude destination
        } else {
            from_component_name.clone()  // If analyzing destination, also exclude source
        },
    });
    
    // v0.1.9.1 BUG FIX: Populate exempt_net_ids to prevent ray-caster from treating
    // the pad's own net as an obstacle during port selection.
    //
    // WITHOUT THIS: Ray projects from Pad_B1's West face and immediately hits
    // Pad_B1's keepout boundary, reporting 0nm clearance and forcing suboptimal
    // port selection (North face instead of West face).
    //
    // WITH THIS: Ray passes through Pad_B1 (exempt net_id=2) and accurately measures
    // clearance to the next real obstacle, allowing optimal port selection.
    let mut router = TopologicalRouter::new(trace_width_nm, space.resolution_nm, clearance_nm);
    
    if let Some(net) = from_net_id {
        router.exempt_net_ids.push(net.raw() as usize);
    }
    if let Some(net) = to_net_id {
        if from_net_id != to_net_id {  // Avoid duplicate if same net
            router.exempt_net_ids.push(net.raw() as usize);
        }
    }
    
    let dx = goal_center.x - start_center.x;
    let dy = goal_center.y - start_center.y;
    
    let port_directions = [
        (CardinalPort::North, RayDirection::North, (0, 1)),
        (CardinalPort::East, RayDirection::East, (1, 0)),
        (CardinalPort::South, RayDirection::South, (0, -1)),
        (CardinalPort::West, RayDirection::West, (-1, 0)),
    ];
    
    let mut analyses: Vec<PortAnalysis> = Vec::new();
    
    for (port, ray_dir, (dir_x, dir_y)) in port_directions {
        let escape_result = calculate_interface_escape(
            interface,
            port,
            EdgeOffset::Center,
            trace_width_nm,
            clearance_nm,
            start_center.z,
            board_bounds.as_ref(),
        );
        
        let has_access_region = escape_result.is_some();
        
        let ray_clearance_nm = if let Some(escape_pt) = escape_result {
            if let Some(ray_hit) = router.project_ray(
                escape_pt.point,
                ray_dir,
                &spatial_index,
                &board_bounds.unwrap(),
            ) {
                let distance = match ray_dir {
                    RayDirection::North | RayDirection::South => {
                        (ray_hit.point.y - escape_pt.point.y).abs()
                    }
                    RayDirection::East | RayDirection::West => {
                        (ray_hit.point.x - escape_pt.point.x).abs()
                    }
                };
                distance
            } else {
                board_bounds.unwrap().max.x.max(board_bounds.unwrap().max.y)
            }
        } else {
            0
        };
        
        let goal_dir_magnitude = ((dx * dx + dy * dy) as f64).sqrt();
        let geometric_alignment = if goal_dir_magnitude > 0.0 {
            ((dx as f64 * dir_x as f64) + (dy as f64 * dir_y as f64)) / goal_dir_magnitude
        } else {
            0.0
        };
        
        analyses.push(PortAnalysis {
            port,
            has_access_region,
            ray_clearance_nm,
            geometric_alignment,
        });
    }
    
    analyses.sort_by(|a, b| {
        b.score()
            .partial_cmp(&a.score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    
   
    for analysis in &analyses {
        eprintln!(
            "  {:?}: clearance={}nm, alignment={:.2}, score={:.3}",
            analysis.port,
            analysis.ray_clearance_nm,
            analysis.geometric_alignment,
            analysis.score()
        );
    }
    
    analyses
        .into_iter()
        .find(|a| a.has_access_region)
        .map(|a| a.port)
        .ok_or_else(|| IrError::NoPathFound {
            net: CompactString::from(format!("{} (no routable escape ports)", entity_name)),
            from_pin: entity_name.clone(),
            to_pin: CompactString::from(format!("{:?}", goal_center)),
        })
}

/// Calculate boundary points using CIR interfaces.
///
/// v0.1.9: This function now queries PhysicalInterface from EntityGraph and uses
/// AccessRegions to compute proper escape points with clearance.
pub fn calculate_boundary_points(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
    trace_width_nm: i64,
) -> BoundaryPoints {
    
    let board_bounds = space.entity_graph.total_bounding_box();

    // Get fabrication constraints (required, no fallbacks)
    let constraints = space
        .fabrication_constraints
        .as_ref()
        .ok_or_else(|| IrError::MissingAsicConstraint {
            message: "Fabrication constraints required for routing".into(),
            hint: "Add 'trace:' block to profile with min_spacing".into(),
        })?;
    
    let clearance_nm = constraints.trace.min_spacing_nm;
   
    let resolve_offset = |spec: &Option<hwc_parser::EdgeOffsetSpec>| -> EdgeOffset {
        match spec {
            Some(hwc_parser::EdgeOffsetSpec::Named(pos)) => match pos {
                hwc_parser::NamedPosition::Top => EdgeOffset::Named(NamedPosition::Top),
                hwc_parser::NamedPosition::Bottom => EdgeOffset::Named(NamedPosition::Bottom),
                hwc_parser::NamedPosition::Center => EdgeOffset::Center,
            },
            Some(hwc_parser::EdgeOffsetSpec::Percentage(p)) => EdgeOffset::Percentage(*p),
            Some(hwc_parser::EdgeOffsetSpec::Measurement(m)) => EdgeOffset::Measurement(*m),
            None => EdgeOffset::Center,
        }
    };

    // v0.2.0: Query layer connection database instead of using bbox center
    // The route must declare which layer it's routing on
    let routing_layer = route.layer.as_ref().ok_or_else(|| IrError::MissingRouteParameter {
        parameter: "layer".into(),
        route: format!("route from {:?} to {:?}", route.from, route.to).into(),
        hint: "Every route must declare the routing layer explicitly.\n\
               Example: route A to B:\n    layer: metal1".into(),
    })?;
    let routing_layer_str = routing_layer.as_str();

    let from_label = crate::ir::routing::helpers::construct_entity_name(&route.from)?;
    let to_label = crate::ir::routing::helpers::construct_entity_name(&route.to)?;

    // Query database for connection points on the routing layer
    let start_conn = space
        .layer_connection_db
        .get_connection_point(&from_label, routing_layer_str)
        .map_err(|e| IrError::InvalidRouteExpression {
            expression: format!("route from {}", from_label),
            reason: format!(
                "Entity '{}' has no connection point on layer '{}': {}",
                from_label, routing_layer, e
            ),
        })?;

    let goal_conn = space
        .layer_connection_db
        .get_connection_point(&to_label, routing_layer_str)
        .map_err(|e| IrError::InvalidRouteExpression {
            expression: format!("route to {}", to_label),
            reason: format!(
                "Entity '{}' has no connection point on layer '{}': {}",
                to_label, routing_layer, e
            ),
        })?;

    // Use connection points from database (not bbox centers!)
    let start_pin_center = Point3D::new(
        start_conn.position_2d.0,
        start_conn.position_2d.1,
        start_conn.z_elevation, // FROM DATABASE!
    );

    let goal_pin_center = Point3D::new(
        goal_conn.position_2d.0,
        goal_conn.position_2d.1,
        goal_conn.z_elevation, // FROM DATABASE!
    );

   

    // v0.1.9: Obstacle-aware auto-port selection
    // Uses topological ray-casting to select escape ports with maximum clearance
    let auto_exit_port = select_routable_port(
        space,
        &route.from,
        start_pin_center,
        goal_pin_center,
        trace_width_nm,
        clearance_nm,
        &from_label,
        &to_label,
    )?;

    let auto_enter_port = select_routable_port(
        space,
        &route.to,
        goal_pin_center,
        start_pin_center,
        trace_width_nm,
        clearance_nm,
        &from_label,
        &to_label,
    )?;

    // v0.1.9 CIR: Query interface by entity name and use interface_escape
    let resolve_point_cir =
        |endpoint: &hwc_parser::RouteEndpointSpec, port: CardinalPort, offset: EdgeOffset, z: i64| {
            let entity_name = match endpoint {
                hwc_parser::RouteEndpointSpec::ComponentPin {
                    component_name,
                    pin_name,
                    ..
                } => format!("{}.{}", component_name, pin_name),
                hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => name.to_string(),
            };

            // Try to get interface for this entity
            if let Some(interface) = space.entity_graph.get_interface_by_entity_name(&entity_name) {
                // Use CIR interface escape system
                calculate_interface_escape(
                    interface,
                    port,
                    offset,
                    trace_width_nm,
                    clearance_nm,
                    z,
                    board_bounds.as_ref(),
                )
            } else {
                None
            }
        };

    let start_esc = if let Some(exit_escape) = &route.exit_escape {
        let port = match exit_escape.port {
            hwc_parser::CardinalDirection::North => CardinalPort::North,
            hwc_parser::CardinalDirection::South => CardinalPort::South,
            hwc_parser::CardinalDirection::East => CardinalPort::East,
            hwc_parser::CardinalDirection::West => CardinalPort::West,
        };
        let offset = resolve_offset(&exit_escape.offset);
        resolve_point_cir(&route.from, port, offset, start_pin_center.z)
    } else {
        resolve_point_cir(
            &route.from,
            auto_exit_port,
            EdgeOffset::Center,
            start_pin_center.z,
        )
    }
    .ok_or_else(|| IrError::NoPathFound {
        net: format!("{} -> {}", from_label, to_label).into(),
        from_pin: from_label.clone(),
        to_pin: to_label.clone(),
    })?;

    let goal_esc = if let Some(enter_escape) = &route.enter_escape {
        let port = match enter_escape.port {
            hwc_parser::CardinalDirection::North => CardinalPort::North,
            hwc_parser::CardinalDirection::South => CardinalPort::South,
            hwc_parser::CardinalDirection::East => CardinalPort::East,
            hwc_parser::CardinalDirection::West => CardinalPort::West,
        };
        let offset = resolve_offset(&enter_escape.offset);
        resolve_point_cir(&route.to, port, offset, goal_pin_center.z)
    } else {
        resolve_point_cir(
            &route.to,
            auto_enter_port,
            EdgeOffset::Center,
            goal_pin_center.z,
        )
    }
    .ok_or_else(|| IrError::NoPathFound {
        net: format!("{} -> {}", from_label, to_label).into(),
        from_pin: from_label.clone(),
        to_pin: to_label.clone(),
    })?;

    Ok((
        start_esc.point,
        goal_esc.point,
        start_esc.direction,
        goal_esc.direction,
    ))
}
