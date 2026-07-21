//! Default routing database for testing and unit test suite.

use crate::geometry::Point3D;

use super::types::RoutingDatabase;

/// Default routing database with typical copper technology parameters.
///
/// Used for unit tests and as a reference implementation. Real designs
/// should use the Salsa-tracked query engine.
pub struct DefaultRoutingDatabase {
    /// Max current density in uA/nm (typical copper: 2 mA/μm² = 2 uA/nm)
    pub max_current_density_ua_per_nm: i64,
}

impl Default for DefaultRoutingDatabase {
    fn default() -> Self {
        Self {
            max_current_density_ua_per_nm: 2,
        }
    }
}

impl RoutingDatabase for DefaultRoutingDatabase {
    fn get_max_current_density_ua_per_nm(&self) -> i64 {
        self.max_current_density_ua_per_nm
    }

    fn get_local_temperature_at(&self, _pos: Point3D) -> i64 {
        300_000 // 300K in millikelvin
    }

    fn get_current_density_at(&self, _pos: Point3D) -> i64 {
        0
    }

    fn get_nearest_parallel_trace_distance(&self, _pos: Point3D) -> i64 {
        i64::MAX
    }

    fn is_in_reference_void(&self, _pos: Point3D) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::BoundingBox;
    use crate::geometry_router::connection_interface::access_region::AccessRegion;
    use crate::geometry_router::connection_interface::capability::InterfaceCapability;
    use crate::geometry_router::connection_interface::geometry::InterfaceGeometry;
    use crate::geometry_router::connection_interface::physical::PhysicalInterface;
    use crate::geometry_router::connection_interface::types::{
        DerivedConstraint, InterfaceId, Normal2D, Orientation,
    };
    use crate::geometry_router::routing_intent::RoutingIntent;
    use crate::netlist::ComponentId;

    #[test]
    fn test_interface_id_roundtrip() {
        let id = InterfaceId::new(42);
        assert_eq!(id.raw(), 42);
    }

    #[test]
    fn test_normal2d_directions() {
        assert_eq!(Normal2D::NORTH.to_unit_direction(), (0, 1));
        assert_eq!(Normal2D::SOUTH.to_unit_direction(), (0, -1));
        assert_eq!(Normal2D::EAST.to_unit_direction(), (1, 0));
        assert_eq!(Normal2D::WEST.to_unit_direction(), (-1, 0));
        assert_eq!(Normal2D::ZERO.to_unit_direction(), (0, 0));
    }

    #[test]
    fn test_interface_geometry_bounding_box() {
        let geom = InterfaceGeometry::Edge {
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(100, 200, 0),
        };
        let bbox = geom.bounding_box();
        assert_eq!(bbox.min, Point3D::new(0, 0, 0));
        assert_eq!(bbox.max, Point3D::new(100, 200, 0));
    }

    #[test]
    fn test_polygon_normals() {
        let verts = vec![
            Point3D::new(0, 0, 0),
            Point3D::new(100, 0, 0),
            Point3D::new(100, 100, 0),
            Point3D::new(0, 100, 0),
        ];
        let geom = InterfaceGeometry::Polygon(verts);
        let normals = geom.derive_normals(Orientation::Derived);
        assert_eq!(normals.len(), 4);
        for n in &normals {
            assert!(
                n.x == 0 || n.y == 0,
                "Expected axis-aligned normal, got ({}, {})",
                n.x,
                n.y
            );
        }
    }

    #[test]
    fn test_capability_constraint_derivation() {
        let db = DefaultRoutingDatabase::default();
        let cap = InterfaceCapability::CarryCurrent { max_ua: 100_000 };
        let constraint = cap.derive_constraint(&db);
        match constraint {
            DerivedConstraint::MinimumTraceWidth(w) => {
                assert_eq!(w, 50_000);
            }
            _ => panic!("Expected MinimumTraceWidth"),
        }
    }

    #[test]
    fn test_access_region_generation() {
        let bbox = BoundingBox::new(Point3D::new(0, 0, 0), Point3D::new(100, 100, 0));
        let regions = AccessRegion::generate_rectangular(&bbox, 50_000, 10_000);
        assert_eq!(regions.len(), 4);
        let normals: Vec<_> = regions.iter().map(|r| r.normal).collect();
        assert!(normals.contains(&Normal2D::NORTH));
        assert!(normals.contains(&Normal2D::SOUTH));
        assert!(normals.contains(&Normal2D::EAST));
        assert!(normals.contains(&Normal2D::WEST));
    }

    #[test]
    fn test_physical_interface_creation() {
        let db = DefaultRoutingDatabase::default();
        let geom = InterfaceGeometry::Edge {
            start: Point3D::new(0, 0, 0),
            end: Point3D::new(100, 0, 0),
        };
        let caps = smallvec::smallvec![InterfaceCapability::CarryCurrent { max_ua: 50_000 }];
        let iface = PhysicalInterface::new(
            InterfaceId::new(1),
            ComponentId::new(42),
            geom,
            caps,
            RoutingIntent::default(),
            &db,
            10_000,
            50_000,
        );
        assert_eq!(iface.id, InterfaceId::new(1));
        assert_eq!(iface.access_regions.len(), 1);
        assert_eq!(iface.boundary_normals.len(), 1);
        assert_eq!(iface.derived_constraints.len(), 1);
    }
}
