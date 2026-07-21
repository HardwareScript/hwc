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
    /// v0.1.9.1: Made public to allow port selection to configure exemptions
    pub exempt_net_ids: Vec<usize>,
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

    /// Route with mandatory perpendicular escape segments (v0.1.9 Zero-Gap Contact Lock).
    ///
    /// This enforces Law 2: Mandatory Perpendicular Escape Segment.
    /// The router is prevented from making a turn immediately at the pad boundary.
    /// The first segment of the path is locked to the port's normal vector for a
    /// user-specified distance (escape_stub_nm).
    ///
    /// # Arguments
    /// * `start` - Contact point (centerline at pad edge + trace_width/2)
    /// * `target` - Contact point (centerline at pad edge + trace_width/2)
    /// * `start_normal` - Normal vector at start (points away from pad)
    /// * `target_normal` - Normal vector at target (points away from pad)
    /// * `escape_stub_nm` - Perpendicular escape distance before turns are allowed (user-declared)
    /// * `obstacles` - Spatial index of obstacles
    /// * `board_bounds` - Board boundaries
    /// * `exempt_net_ids` - Net IDs to exempt from collision checks
    pub fn route_with_perpendicular_escape(
        &self,
        start: Point3D,
        target: Point3D,
        start_normal: crate::geometry_router::connection_interface::Normal2D,
        target_normal: crate::geometry_router::connection_interface::Normal2D,
        escape_stub_nm: i64,
        obstacles: &DynamicSpatialIndex,
        board_bounds: &BoundingBox,
        exempt_net_ids: &[usize],
    ) -> Option<TopologicalPath> {
        const SCALE: i64 = 1_000_000_000;

        // If escape_stub is 0, no perpendicular escape required - route directly
        if escape_stub_nm == 0 {
            eprintln!("[PERPENDICULAR ESCAPE] escape_stub=0nm - routing with immediate turns allowed");
            let router = TopologicalRouter {
                trace_width_nm: self.trace_width_nm,
                layer_prefer_horizontal: self.layer_prefer_horizontal,
                track_pitch_nm: self.track_pitch_nm,
                min_clearance_nm: self.min_clearance_nm,
                exempt_net_ids: exempt_net_ids.to_vec(),
            };
            return router.route(start, target, obstacles, board_bounds);
        }

        eprintln!("[PERPENDICULAR ESCAPE] escape_stub={}nm (user-declared)", escape_stub_nm);
        eprintln!("  start=({},{},{}) normal=({},{})", start.x, start.y, start.z, start_normal.x, start_normal.y);
        eprintln!("  target=({},{},{}) normal=({},{})", target.x, target.y, target.z, target_normal.x, target_normal.y);

        // Generate mandatory start escape point
        let start_escape = Point3D::new(
            start.x + (start_normal.x as i64 * escape_stub_nm) / SCALE,
            start.y + (start_normal.y as i64 * escape_stub_nm) / SCALE,
            start.z,
        );

        // Generate mandatory target escape point
        let target_escape = Point3D::new(
            target.x + (target_normal.x as i64 * escape_stub_nm) / SCALE,
            target.y + (target_normal.y as i64 * escape_stub_nm) / SCALE,
            target.z,
        );

        eprintln!("  start_escape=({},{},{})", start_escape.x, start_escape.y, start_escape.z);
        eprintln!("  target_escape=({},{},{})", target_escape.x, target_escape.y, target_escape.z);

        // Create a router with exemptions
        let router = TopologicalRouter {
            trace_width_nm: self.trace_width_nm,
            layer_prefer_horizontal: self.layer_prefer_horizontal,
            track_pitch_nm: self.track_pitch_nm,
            min_clearance_nm: self.min_clearance_nm,
            exempt_net_ids: exempt_net_ids.to_vec(),
        };

        // Run the topological pathfinder from start_escape to target_escape
        if let Some(mut intermediate_path) = router.route(start_escape, target_escape, obstacles, board_bounds) {
            // Native Splice: prepend the start contact point and append the target contact point
            let mut final_waypoints = vec![start];
            final_waypoints.append(&mut intermediate_path.waypoints);
            final_waypoints.push(target);

            // Recalculate total length
            let total_length = final_waypoints
                .windows(2)
                .map(|w| {
                    let dx = w[1].x - w[0].x;
                    let dy = w[1].y - w[0].y;
                    let dz = w[1].z - w[0].z;
                    ((dx * dx + dy * dy + dz * dz) as f64).sqrt() as i64
                })
                .sum();

            eprintln!("  ✅ Perpendicular escape routing succeeded: {} waypoints, total_length={}nm",
                final_waypoints.len(), total_length);

            Some(TopologicalPath {
                waypoints: final_waypoints,
                total_length,
            })
        } else {
            eprintln!("  ❌ Perpendicular escape routing failed: no path found between escape points");
            None
        }
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
