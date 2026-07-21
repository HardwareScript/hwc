//! Interface geometry types with normal derivation.

use crate::geometry::{BoundingBox, Point3D};

use super::types::{Normal2D, Orientation};

/// Physical geometry of a component interface.
///
/// Different geometries map to different escape strategies:
/// - `Point`: Single-point contact (e.g., solder ball)
/// - `Edge`: Linear edge contact (e.g., flat pad edge)
/// - `Polygon`: Multi-vertex contact (e.g., complex pad shape)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InterfaceGeometry {
    /// Single-point contact (radial escape)
    Point(Point3D),
    /// Linear edge contact (cardinal/normal escape)
    Edge { start: Point3D, end: Point3D },
    /// Multi-vertex polygon contact (per-edge normal escape)
    Polygon(Vec<Point3D>),
}

impl InterfaceGeometry {
    /// Compute the bounding box of this geometry.
    pub fn bounding_box(&self) -> BoundingBox {
        match self {
            Self::Point(p) => BoundingBox::new(*p, *p),
            Self::Edge { start, end } => BoundingBox::new(
                Point3D::new(start.x.min(end.x), start.y.min(end.y), start.z.min(end.z)),
                Point3D::new(start.x.max(end.x), start.y.max(end.y), start.z.max(end.z)),
            ),
            Self::Polygon(vertices) => Self::polygon_bbox(vertices),
        }
    }

    /// Derive outward perpendicular normal vectors for this geometry.
    ///
    /// Returns one `Normal2D` per polygon edge.
    /// Uses integer-only math for deterministic, bit-identical results.
    pub fn derive_normals(&self, orientation: Orientation) -> Vec<Normal2D> {
        match orientation {
            Orientation::None => vec![],
            Orientation::Explicit(normal) => vec![normal],
            Orientation::Derived => self.derive_normals_from_edges(),
        }
    }

    fn derive_normals_from_edges(&self) -> Vec<Normal2D> {
        match self {
            Self::Point(_) => vec![],
            Self::Edge { start, end } => vec![Self::compute_edge_normal(start, end)],
            Self::Polygon(vertices) => {
                if vertices.len() < 2 {
                    return vec![];
                }
                let n = vertices.len();
                (0..n)
                    .map(|i| Self::compute_edge_normal(&vertices[i], &vertices[(i + 1) % n]))
                    .collect()
            }
        }
    }

    /// Compute outward normal for a single edge using perpendicular rotation.
    ///
    /// For CCW-wound polygons, rotate the edge direction 90° counterclockwise:
    /// - Edge vector: (dx, dy)
    /// - Outward normal: (dy, -dx) [perpendicular, pointing outward]
    ///
    /// Uses fixed-point arithmetic for determinism.
    fn compute_edge_normal(start: &Point3D, end: &Point3D) -> Normal2D {
        let dx = (end.x - start.x) as i128;
        let dy = (end.y - start.y) as i128;
        let d2 = (dx * dx + dy * dy) as u128;

        if d2 == 0 {
            return Normal2D::ZERO;
        }

        let len = crate::geometry_router::geometry_math::integer_sqrt(d2) as i128;
        if len == 0 {
            return Normal2D::ZERO;
        }

        // Corrected: For CCW polygon, outward normal is (dy, -dx)
        let nx = ((dy * Normal2D::SCALE as i128) / len) as i32;
        let ny = ((-dx * Normal2D::SCALE as i128) / len) as i32;
        Normal2D { x: nx, y: ny }
    }

    fn polygon_bbox(vertices: &[Point3D]) -> BoundingBox {
        let mut min_x = i64::MAX;
        let mut min_y = i64::MAX;
        let mut min_z = i64::MAX;
        let mut max_x = i64::MIN;
        let mut max_y = i64::MIN;
        let mut max_z = i64::MIN;
        for v in vertices {
            min_x = min_x.min(v.x);
            min_y = min_y.min(v.y);
            min_z = min_z.min(v.z);
            max_x = max_x.max(v.x);
            max_y = max_y.max(v.y);
            max_z = max_z.max(v.z);
        }
        BoundingBox::new(
            Point3D::new(min_x, min_y, min_z),
            Point3D::new(max_x, max_y, max_z),
        )
    }
}
