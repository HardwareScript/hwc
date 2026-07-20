use crate::ir::errors::IrError;
use hwc_engine::{HardwareSpace, Point3D};

/// Resolve a ResolvedRoute's EntityId endpoints to boundary coordinates.
pub fn resolve_route_boundary_points(
    space: &HardwareSpace,
    route: &super::super::types::ResolvedRoute,
    trace_width_nm: i64,
) -> Result<(Point3D, Point3D), IrError> {
    use hwc_engine::geometry::BoundingBox;
    use hwc_engine::geometry::EntityId;
    use hwc_engine::geometry_router::port_escape::{
        calculate_rect_escape, CardinalPort, EdgeOffset as EngineEdgeOffset,
    };

    let board_bounds = space.entity_graph.total_bounding_box();

    let to_engine_port = |dir: super::super::types::CardinalDirection| match dir {
        super::super::types::CardinalDirection::North => CardinalPort::North,
        super::super::types::CardinalDirection::South => CardinalPort::South,
        super::super::types::CardinalDirection::East => CardinalPort::East,
        super::super::types::CardinalDirection::West => CardinalPort::West,
    };

    let to_engine_offset = |off: super::super::types::EdgeOffset| match off {
        super::super::types::EdgeOffset::Center => EngineEdgeOffset::Center,
        super::super::types::EdgeOffset::Percentage(p) => EngineEdgeOffset::Percentage(p),
        super::super::types::EdgeOffset::MeasurementNm(nm) => EngineEdgeOffset::Measurement(nm),
    };

    let resolve_bbox = |entity_id: EntityId, label: &str| -> Result<BoundingBox, IrError> {
        space
            .entity_graph
            .get_entity_data(entity_id)
            .map(|d| d.bbox)
            .map_err(|_| IrError::UnresolvedEndpoint {
                endpoint: format!("Entity {:?} ({})", entity_id, label),
                span: miette::SourceSpan::from(0),
                help_message: format!(
                    "EntityId {:?} not found in EntityGraph. \
                         Ensure the entity is registered before routing.",
                    entity_id
                ),
            })
    };

    let resolve_pin_z = |entity_id: EntityId| -> Result<i64, IrError> {
        let data = space.entity_graph.get_entity_data(entity_id).map_err(|_| {
            IrError::UnresolvedEndpoint {
                endpoint: format!("Entity {:?}", entity_id),
                span: miette::SourceSpan::from(0),
                help_message: "EntityId not found in EntityGraph.".into(),
            }
        })?;
        Ok((data.bbox.min.z + data.bbox.max.z) / 2)
    };

    let from_bbox = resolve_bbox(route.from, &route.net_name)?;
    let to_bbox = resolve_bbox(route.to, &route.net_name)?;
    let start_z = resolve_pin_z(route.from)?;
    let goal_z = resolve_pin_z(route.to)?;

    let start_escape = calculate_rect_escape(
        &from_bbox,
        to_engine_port(route.exit_escape.port),
        to_engine_offset(route.exit_escape.offset),
        trace_width_nm,
        trace_width_nm / 2,
        start_z,
        board_bounds.as_ref(),
    );

    let goal_escape = calculate_rect_escape(
        &to_bbox,
        to_engine_port(route.enter_escape.port),
        to_engine_offset(route.enter_escape.offset),
        trace_width_nm,
        trace_width_nm / 2,
        goal_z,
        board_bounds.as_ref(),
    );

    Ok((start_escape.point, goal_escape.point))
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
