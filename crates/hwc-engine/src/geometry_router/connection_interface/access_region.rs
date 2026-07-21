//! Pre-computed access region types and generation methods.

use crate::geometry::{BoundingBox, Point3D};
use smallvec::{smallvec, SmallVec};

use super::types::Normal2D;

/// Pre-computed approach zone for an interface.
///
/// Access regions are generated once during interface creation and cached
/// as immutable data. They describe how the pathfinder can approach the
/// interface without recomputing geometry on every routing query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRegion {
    /// Entry point where the trace docks to the interface
    pub entry_point: Point3D,
    /// Outward normal direction of this access region
    pub normal: Normal2D,
    /// Approach corridor bounding box (Minkowski-inflated)
    pub corridor: BoundingBox,
    /// Priority for candidate selection (lower = preferred)
    pub priority: u32,
}

impl std::hash::Hash for AccessRegion {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.entry_point.hash(state);
        self.normal.hash(state);
        self.corridor.min.x.hash(state);
        self.corridor.min.y.hash(state);
        self.corridor.min.z.hash(state);
        self.corridor.max.x.hash(state);
        self.corridor.max.y.hash(state);
        self.corridor.max.z.hash(state);
        self.priority.hash(state);
    }
}

impl AccessRegion {
    /// Generate an access region for an edge geometry.
    ///
    /// The entry_point is offset in the normal direction to sit just outside
    /// the pad boundary, ensuring traces connect at the edge rather than penetrating
    /// into the pad interior.
    pub fn generate(
        start: &Point3D,
        end: &Point3D,
        normal: &Normal2D,
        escape_stub_length_nm: i64,
        trace_width_nm: i64,
    ) -> Self {
        // Calculate edge midpoint
        let edge_center_x = (start.x + end.x) / 2;
        let edge_center_y = (start.y + end.y) / 2;
        let edge_center_z = (start.z + end.z) / 2;

        // Offset the entry point outward in the normal direction
        // Use half trace width as the offset to position just outside the pad
        let (dx, dy) = normal.to_unit_direction();
        let clearance_offset = trace_width_nm / 2;

        let entry_point = Point3D::new(
            edge_center_x + dx * clearance_offset,
            edge_center_y + dy * clearance_offset,
            edge_center_z,
        );

        let min_x = start.x.min(end.x);
        let max_x = start.x.max(end.x);
        let min_y = start.y.min(end.y);
        let max_y = start.y.max(end.y);

        let half_trace = trace_width_nm / 2;

        let corridor = BoundingBox::new(
            Point3D::new(
                min_x - half_trace + dx * escape_stub_length_nm,
                min_y - half_trace + dy * escape_stub_length_nm,
                start.z.min(end.z),
            ),
            Point3D::new(
                max_x + half_trace + dx * escape_stub_length_nm,
                max_y + half_trace + dy * escape_stub_length_nm,
                start.z.max(end.z),
            ),
        );

        Self {
            entry_point,
            normal: *normal,
            corridor,
            priority: 0,
        }
    }

    /// Generate four access regions for a rectangular bounding box (N, S, E, W).
    pub fn generate_rectangular(
        bbox: &BoundingBox,
        escape_stub_length_nm: i64,
        trace_width_nm: i64,
    ) -> SmallVec<[Self; 4]> {
        smallvec![
            Self::generate(
                &Point3D::new(bbox.min.x, bbox.max.y, bbox.min.z),
                &Point3D::new(bbox.max.x, bbox.max.y, bbox.min.z),
                &Normal2D::NORTH,
                escape_stub_length_nm,
                trace_width_nm,
            ),
            Self::generate(
                &Point3D::new(bbox.min.x, bbox.min.y, bbox.min.z),
                &Point3D::new(bbox.max.x, bbox.min.y, bbox.min.z),
                &Normal2D::SOUTH,
                escape_stub_length_nm,
                trace_width_nm,
            ),
            Self::generate(
                &Point3D::new(bbox.max.x, bbox.min.y, bbox.min.z),
                &Point3D::new(bbox.max.x, bbox.max.y, bbox.min.z),
                &Normal2D::EAST,
                escape_stub_length_nm,
                trace_width_nm,
            ),
            Self::generate(
                &Point3D::new(bbox.min.x, bbox.min.y, bbox.min.z),
                &Point3D::new(bbox.min.x, bbox.max.y, bbox.min.z),
                &Normal2D::WEST,
                escape_stub_length_nm,
                trace_width_nm,
            ),
        ]
    }

    /// Generate one access region per polygon edge.
    pub fn generate_polygon(
        vertices: &[Point3D],
        normals: &[Normal2D],
        escape_stub_length_nm: i64,
        trace_width_nm: i64,
    ) -> SmallVec<[Self; 8]> {
        let mut regions = smallvec::smallvec![];
        if vertices.len() < 2 || normals.is_empty() {
            return regions;
        }
        for (i, normal) in normals.iter().enumerate() {
            let start = &vertices[i];
            let end = &vertices[(i + 1) % vertices.len()];
            regions.push(Self::generate(
                start,
                end,
                normal,
                escape_stub_length_nm,
                trace_width_nm,
            ));
        }
        regions
    }
}
