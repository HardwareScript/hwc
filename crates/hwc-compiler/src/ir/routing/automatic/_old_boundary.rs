//! Boundary point calculation for automatic routing.
//!
//! Computes exact boundary points and escape directions for routing
//! between component pins using topological ray-casting.

use crate::ir::errors::IrError;
use crate::ir::routing::helpers::get_pin_positions;
use hwc_engine::{HardwareSpace, Point3D};

/// Result of boundary-point calculation: two 3D points (entry/exit) plus the
/// two escape directions expressed as integer (dx, dy) offsets.
pub(crate) type BoundaryPoints = Result<(Point3D, Point3D, (i64, i64), (i64, i64)), IrError>;

/// Calculate boundary points and exit directions for routing.
///
/// Computes the exact points on component pin boundaries where traces should
/// connect, along with the escape directions for routing away from the pins.
pub fn calculate_boundary_points(
    space: &HardwareSpace,
    route: &hwc_parser::Route,
    trace_width_nm: i64,
) -> BoundaryPoints {
    use hwc_engine::geometry_router::port_escape::{
        calculate_rect_escape, CardinalPort, EdgeOffset, NamedPosition,
    };

    let board_bounds = space.entity_graph.total_bounding_box();

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

    let (start_pin_center, goal_pin_center) = get_pin_positions(space, route)?;

    let dx = goal_pin_center.x - start_pin_center.x;
    let dy = goal_pin_center.y - start_pin_center.y;

    let auto_exit_port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
        if dy > 0 {
            CardinalPort::North
        } else {
            CardinalPort::South
        }
    } else if dx.abs() > 0 {
        if dx > 0 {
            CardinalPort::East
        } else {
            CardinalPort::West
        }
    } else if dy > 0 {
        CardinalPort::North
    } else {
        CardinalPort::South
    };

    let auto_enter_port = if dy.abs() >= 1_000_000 && dy.abs() > dx.abs() / 4 {
        if dy > 0 {
            CardinalPort::South
        } else {
            CardinalPort::North
        }
    } else if dx.abs() > 0 {
        if dx > 0 {
            CardinalPort::West
        } else {
            CardinalPort::East
        }
    } else if dy > 0 {
        CardinalPort::South
    } else {
        CardinalPort::North
    };

    let resolve_point = |endpoint: &hwc_parser::RouteEndpointSpec,
                         port: CardinalPort,
                         offset: EdgeOffset,
                         z: i64| {
        let bbox_opt = match endpoint {
            hwc_parser::RouteEndpointSpec::ComponentPin {
                component_name,
                pin_name,
                ..
            } => space
                .entity_graph
                .get_component_pin_bbox(component_name.as_str(), pin_name.as_str()),
            hwc_parser::RouteEndpointSpec::SpaceEntity { name, .. } => {
                space.entity_graph.get_space_entity_bbox(name.as_str())
            }
        };

        bbox_opt.map(|bbox| {
            let boundary_clearance = trace_width_nm / 2;
            calculate_rect_escape(
                &bbox,
                port,
                offset,
                trace_width_nm,
                boundary_clearance,
                z,
                board_bounds.as_ref(),
            )
        })
    };

    let from_label = crate::ir::routing::helpers::construct_entity_name(&route.from)?;
    let to_label = crate::ir::routing::helpers::construct_entity_name(&route.to)?;

    let start_esc = if let Some(exit_escape) = &route.exit_escape {
        let port = match exit_escape.port {
            hwc_parser::CardinalDirection::North => CardinalPort::North,
            hwc_parser::CardinalDirection::South => CardinalPort::South,
            hwc_parser::CardinalDirection::East => CardinalPort::East,
            hwc_parser::CardinalDirection::West => CardinalPort::West,
        };
        let offset = resolve_offset(&exit_escape.offset);
        resolve_point(&route.from, port, offset, start_pin_center.z)
    } else {
        resolve_point(
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
        resolve_point(&route.to, port, offset, goal_pin_center.z)
    } else {
        resolve_point(
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
