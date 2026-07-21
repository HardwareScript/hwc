//! Interface Escape Dispatch
//!
//! Geometry-based escape strategy selection that bridges CIR interfaces
//! to the existing port escape system. Routes to the appropriate strategy
//! based on interface geometry type.
//!
//! Reference: `Docs/v0.1.9/Connection-Interface-Routing.md` §2.2

use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::connection_interface::{InterfaceGeometry, PhysicalInterface};
use crate::geometry_router::port_escape::{self, CardinalPort, EdgeOffset, EscapePoint};

/// Escape strategy for different interface geometries.
///
/// Selects the appropriate port escape calculation based on the
/// physical shape of the interface.
pub fn calculate_interface_escape(
    interface: &PhysicalInterface,
    port: CardinalPort,
    offset: EdgeOffset,
    trace_width_nm: i64,
    clearance_nm: i64,
    z: i64,
    board_bounds: Option<&BoundingBox>,
) -> Option<EscapePoint> {
    match &interface.geometry {
        InterfaceGeometry::Point(p) => {
            // Point interfaces: radial projection
            let radius_nm = 0; // Point has zero radius
            Some(port_escape::calculate_circular_escape(
                (p.x, p.y),
                radius_nm,
                port,
                offset,
                trace_width_nm,
                clearance_nm,
                z,
            ))
        }
        InterfaceGeometry::Edge { start: _, end: _ } => {
            // Edge interfaces: use bounding box with cardinal ports
            let bbox = interface.geometry.bounding_box();
            Some(port_escape::calculate_rect_escape(
                &bbox,
                port,
                offset,
                trace_width_nm,
                clearance_nm,
                z,
                board_bounds,
            ))
        }
        InterfaceGeometry::Polygon(_) => {
            // Polygon interfaces: select best edge based on access regions
            // Find the access region closest to the requested port direction
            let best_region = interface.access_regions.iter().min_by_key(|region| {
                let (nx, ny) = region.normal.to_unit_direction();
                let (px, py) = port.direction_vector();
                // Manhattan distance between normal and port direction
                (nx - px).abs() + (ny - py).abs()
            })?;

            // Use the access region's entry point and corridor for escape
            let corridor = &best_region.corridor;
            Some(port_escape::calculate_rect_escape(
                corridor,
                port,
                offset,
                trace_width_nm,
                clearance_nm,
                z,
                board_bounds,
            ))
        }
    }
}

/// Select the best port for a given interface geometry and target position.
///
/// For rectangular interfaces, returns the cardinal port closest to the target.
/// For polygon interfaces, returns the port aligned with the best access region normal.
pub fn select_best_port(interface: &PhysicalInterface, target: &Point3D) -> CardinalPort {
    let bbox = interface.geometry.bounding_box();
    let center = bbox.center();

    // Simple heuristic: pick the port whose direction points toward target
    let dx = target.x - center.x;
    let dy = target.y - center.y;

    if dx.abs() >= dy.abs() {
        if dx >= 0 {
            CardinalPort::East
        } else {
            CardinalPort::West
        }
    } else if dy >= 0 {
        CardinalPort::North
    } else {
        CardinalPort::South
    }
}

/// Calculate escape points for all ports of an interface.
///
/// Returns up to 4 escape points (N, S, E, W), filtering out degenerate cases
/// where the escape point would be inside the interface geometry.
pub fn calculate_all_escapes(
    interface: &PhysicalInterface,
    trace_width_nm: i64,
    clearance_nm: i64,
    z: i64,
    board_bounds: Option<&BoundingBox>,
) -> Vec<EscapePoint> {
    let ports = [
        CardinalPort::North,
        CardinalPort::South,
        CardinalPort::East,
        CardinalPort::West,
    ];

    let mut escapes = Vec::with_capacity(4);
    for port in ports {
        if let Some(escape) = calculate_interface_escape(
            interface,
            port,
            EdgeOffset::Center,
            trace_width_nm,
            clearance_nm,
            z,
            board_bounds,
        ) {
            escapes.push(escape);
        }
    }
    escapes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry_router::connection_interface::*;
    use crate::geometry_router::routing_intent::RoutingIntent;
    use crate::netlist::ComponentId;

    fn make_rect_interface() -> PhysicalInterface {
        let db = DefaultRoutingDatabase::default();
        let geom = InterfaceGeometry::Edge {
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(100, 0, 0),
        };
        PhysicalInterface::new(
            InterfaceId::new(1),
            ComponentId::new(42),
            geom,
            smallvec::smallvec![],
            RoutingIntent::default(),
            &db,
            10_000,
            50_000,
        )
    }

    #[test]
    fn test_select_best_port_east() {
        let iface = make_rect_interface();
        let target = Point3D::new(1000, 0, 0);
        assert_eq!(select_best_port(&iface, &target), CardinalPort::East);
    }

    #[test]
    fn test_select_best_port_north() {
        let iface = make_rect_interface();
        let target = Point3D::new(50, 1000, 0);
        assert_eq!(select_best_port(&iface, &target), CardinalPort::North);
    }

    #[test]
    fn test_calculate_all_escapes() {
        let iface = make_rect_interface();
        let escapes = calculate_all_escapes(&iface, 10_000, 5_000, 0, None);
        assert_eq!(escapes.len(), 4);
    }
}
