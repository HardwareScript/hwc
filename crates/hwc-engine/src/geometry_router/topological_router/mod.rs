pub(crate) mod collision;
pub(crate) mod path_builder;
pub(crate) mod ray;
pub mod types;

use crate::geometry::{BoundingBox, Point3D};
use crate::geometry_router::spatial_index::DynamicSpatialIndex;
use rustc_hash::FxHashMap;

pub use types::{RayDirection, RayIntersection, SearchRay, TopologicalPath};

/// The Topological Line-Search Router.
///
/// Projects orthogonal search rays from start and target ports.
/// Uses the Axis-Aligned Slab Method for O(log N) ray-AABB intersection.
/// When rays collide with obstacles, it bends at 90 or 135 degrees.
pub struct TopologicalRouter {
    /// Width of routing traces in nanometers
    pub trace_width_nm: i64,
    /// Preferred routing direction per layer (true = horizontal)
    pub layer_prefer_horizontal: bool,
    /// Track pitch for grid snapping
    pub track_pitch_nm: i64,
    /// Minimum clearance from obstacles (Minkowski inflation).
    pub min_clearance_nm: i64,
    /// Net IDs to exempt from collision checks (e.g., start/goal pads).
    exempt_net_ids: Vec<usize>,
}

impl TopologicalRouter {
    /// Create a new router with required clearance parameter.
    pub fn new(trace_width_nm: i64, track_pitch_nm: i64, min_clearance_nm: i64) -> Self {
        Self {
            trace_width_nm,
            layer_prefer_horizontal: true,
            track_pitch_nm,
            min_clearance_nm,
            exempt_net_ids: Vec::new(),
        }
    }

    /// Route with entity exemptions to prevent start/goal self-collision.
    pub fn route_with_exemptions(
        &self,
        start: Point3D,
        target: Point3D,
        obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
        exempt_net_ids: &[usize],
    ) -> Option<TopologicalPath> {
        let router = TopologicalRouter {
            trace_width_nm: self.trace_width_nm,
            layer_prefer_horizontal: self.layer_prefer_horizontal,
            track_pitch_nm: self.track_pitch_nm,
            min_clearance_nm: self.min_clearance_nm,
            exempt_net_ids: exempt_net_ids.to_vec(),
        };

        router.route(start, target, obstacles, board_bounds)
    }

    /// Route from start to target using topological line-search.
    pub fn route(
        &self,
        start: Point3D,
        target: Point3D,
        obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
    ) -> Option<TopologicalPath> {
        if start == target {
            return Some(TopologicalPath {
                waypoints: vec![start],
                total_length: 0,
            });
        }

        let start_rays = self.project_all_rays(start, obstacles, board_bounds);
        let target_rays = self.project_all_rays(target, obstacles, board_bounds);

        // Try direct orthogonal connection first (L-shape or straight)
        if let Some(path) = self.try_direct_path(start, target, obstacles, board_bounds) {
            return Some(path);
        }

        let mut best_path: Option<TopologicalPath> = None;
        let mut best_length = i64::MAX;

        // Try intersecting ray pairs (one from start, one from target)
        for s_ray in &start_rays {
            for t_ray in &target_rays {
                if let Some(meeting) = Self::find_ray_intersection(s_ray, t_ray) {
                    if self.point_in_obstacle(meeting, obstacles) {
                        continue;
                    }
                    if let Some(path) = self.build_path_from_rays(types::RayPathQuery {
                        start,
                        target,
                        s_ray,
                        t_ray,
                        meeting,
                        obstacles,
                        board_bounds,
                    }) {
                        if path.total_length < best_length {
                            best_length = path.total_length;
                            best_path = Some(path);
                        }
                    }
                }
            }
        }

        // Try perpendicular ray pairs that share an axis
        if best_path.is_none() {
            for s_ray in &start_rays {
                for t_ray in &target_rays {
                    if s_ray.direction == t_ray.direction {
                        continue;
                    }
                    if s_ray.direction.is_horizontal() == t_ray.direction.is_horizontal() {
                        continue;
                    }
                    let meeting = Point3D::new(
                        if t_ray.direction.is_horizontal() {
                            t_ray.origin.x
                        } else {
                            s_ray.origin.x
                        },
                        if s_ray.direction.is_horizontal() {
                            s_ray.origin.y
                        } else {
                            t_ray.origin.y
                        },
                        start.z,
                    );
                    if self.point_in_obstacle(meeting, obstacles) {
                        continue;
                    }
                    if let Some(path) =
                        self.build_z_path(start, target, meeting, obstacles, board_bounds)
                    {
                        if path.total_length < best_length {
                            best_length = path.total_length;
                            best_path = Some(path);
                        }
                    }
                }
            }
        }

        // Try parallel ray pairs to find 2-bend (Z-shape) paths
        if best_path.is_none() {
            for s_ray in &start_rays {
                for t_ray in &target_rays {
                    if s_ray.direction.is_horizontal() == t_ray.direction.is_horizontal() {
                        let path = if s_ray.direction.is_horizontal() {
                            self.try_parallel_horizontal(
                                start,
                                target,
                                s_ray,
                                t_ray,
                                obstacles,
                                board_bounds,
                            )
                        } else {
                            self.try_parallel_vertical(
                                start,
                                target,
                                s_ray,
                                t_ray,
                                obstacles,
                                board_bounds,
                            )
                        };
                        if let Some(path) = path {
                            if path.total_length < best_length {
                                best_length = path.total_length;
                                best_path = Some(path);
                            }
                        }
                    }
                }
            }
        }

        best_path
    }
}

/// Find open-space waypoints by expanding from a point in cardinal directions.
pub fn expand_from_point(
    origin: Point3D,
    obstacles: &DynamicSpatialIndex,
    board_bounds: &BoundingBox,
    trace_width_nm: i64,
    min_clearance_nm: i64,
) -> FxHashMap<RayDirection, Point3D> {
    let mut result = FxHashMap::default();
    let router = TopologicalRouter::new(trace_width_nm, trace_width_nm, min_clearance_nm);

    let directions = [
        RayDirection::North,
        RayDirection::South,
        RayDirection::East,
        RayDirection::West,
    ];

    for &dir in &directions {
        let max_dist = match dir {
            RayDirection::North => (board_bounds.max.y - origin.y).max(0),
            RayDirection::South => (origin.y - board_bounds.min.y).max(0),
            RayDirection::East => (board_bounds.max.x - origin.x).max(0),
            RayDirection::West => (origin.x - board_bounds.min.x).max(0),
        };

        if max_dist <= 0 {
            continue;
        }

        let farthest = match router.project_ray(origin, dir, obstacles, board_bounds) {
            Some(intersection) => {
                let margin = trace_width_nm;
                match dir {
                    RayDirection::North => {
                        Point3D::new(origin.x, intersection.point.y - margin, origin.z)
                    }
                    RayDirection::South => {
                        Point3D::new(origin.x, intersection.point.y + margin, origin.z)
                    }
                    RayDirection::East => {
                        Point3D::new(intersection.point.x - margin, origin.y, origin.z)
                    }
                    RayDirection::West => {
                        Point3D::new(intersection.point.x + margin, origin.y, origin.z)
                    }
                }
            }
            None => match dir {
                RayDirection::North => Point3D::new(origin.x, board_bounds.max.y, origin.z),
                RayDirection::South => Point3D::new(origin.x, board_bounds.min.y, origin.z),
                RayDirection::East => Point3D::new(board_bounds.max.x, origin.y, origin.z),
                RayDirection::West => Point3D::new(board_bounds.min.x, origin.y, origin.z),
            },
        };

        result.insert(dir, farthest);
    }

    result
}
