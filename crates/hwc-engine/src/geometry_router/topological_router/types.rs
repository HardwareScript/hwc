use crate::geometry::{BoundingBox, Point3D};

/// Direction of a ray projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RayDirection {
    North, // +Y
    South, // -Y
    East,  // +X
    West,  // -X
}

impl RayDirection {
    #[inline]
    pub fn perpendicular(self) -> &'static [RayDirection] {
        match self {
            RayDirection::North | RayDirection::South => &[RayDirection::East, RayDirection::West],
            RayDirection::East | RayDirection::West => &[RayDirection::North, RayDirection::South],
        }
    }

    #[inline]
    pub fn is_horizontal(self) -> bool {
        matches!(self, RayDirection::East | RayDirection::West)
    }
}

/// A search ray projected from a point in a cardinal direction.
#[derive(Clone, Debug)]
pub struct SearchRay {
    pub origin: Point3D,
    pub direction: RayDirection,
    /// Maximum distance the ray can travel (board bounds)
    pub max_distance: i64,
}

/// An intersection between a ray and an obstacle AABB.
#[derive(Clone, Debug)]
pub struct RayIntersection {
    /// The exact coordinate where the ray hits the obstacle
    pub point: Point3D,
    /// Distance from ray origin to intersection
    pub distance: i64,
    /// The obstacle that was hit
    pub obstacle: BoundingBox,
}

/// A routed path consisting of orthogonal segments.
#[derive(Clone, Debug)]
pub struct TopologicalPath {
    pub waypoints: Vec<Point3D>,
    pub total_length: i64,
}

/// Parameters for path building from ray pairs.
pub(crate) struct RayPathQuery<'a> {
    pub start: Point3D,
    pub target: Point3D,
    pub s_ray: &'a SearchRay,
    pub t_ray: &'a SearchRay,
    pub meeting: Point3D,
    pub obstacles: &'a crate::geometry_router::spatial_index::DynamicSpatialIndex,
    pub board_bounds: &'a BoundingBox,
}
